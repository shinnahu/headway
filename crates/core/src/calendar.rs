//! Reading a calendar from its private link (Google Calendar calls it the
//! "Secret address in iCal format"). Read-only by nature: the link only lets
//! us download the events.
//!
//! The link is a secret: it is kept in the OS credential store, never written
//! to the config file, never sent to the UI, and never put in an error message.

use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::Serialize;

use crate::{Error, Result};

const MAX_BYTES: usize = 20 * 1024 * 1024;
/// Upper bound on the occurrences of one repeating event inside the window.
const MAX_INSTANCES: u16 = 400;

/// A point in time as calendars write it.
#[derive(Clone, Debug, PartialEq)]
enum When {
    /// An all-day date.
    Date(NaiveDate),
    Utc(DateTime<Utc>),
    /// A wall-clock time in a named time zone.
    Zoned(NaiveDateTime, chrono_tz::Tz),
    /// A wall-clock time with no zone: read in this computer's time zone.
    Floating(NaiveDateTime),
}

/// Turns a wall-clock time into a real instant, also when daylight saving
/// makes that time happen twice (the first is used) or not at all (the hour after).
fn resolve<Z: TimeZone>(tz: &Z, naive: &NaiveDateTime) -> DateTime<Z> {
    match tz.from_local_datetime(naive) {
        chrono::LocalResult::Single(t) => t,
        chrono::LocalResult::Ambiguous(first, _) => first,
        chrono::LocalResult::None => tz
            .from_local_datetime(&(*naive + Duration::hours(1)))
            .earliest()
            .unwrap_or_else(|| tz.from_utc_datetime(naive)),
    }
}

fn midnight(d: &NaiveDate) -> NaiveDateTime {
    d.and_time(NaiveTime::MIN)
}

impl When {
    fn is_date(&self) -> bool {
        matches!(self, When::Date(_))
    }

    /// The instant this stands for. All-day dates count as midnight UTC, which
    /// is only used to tell occurrences apart, never shown.
    fn instant(&self) -> DateTime<Utc> {
        match self {
            When::Date(d) => Utc.from_utc_datetime(&midnight(d)),
            When::Utc(t) => *t,
            When::Zoned(n, tz) => resolve(tz, n).with_timezone(&Utc),
            When::Floating(n) => resolve(&Local, n).with_timezone(&Utc),
        }
    }

    /// The same moment in the form the recurrence library expands from. It
    /// keeps the time zone, so a 9:00 meeting stays at 9:00 across daylight saving.
    fn for_rule(&self) -> DateTime<rrule::Tz> {
        match self {
            When::Date(d) => rrule::Tz::UTC.from_utc_datetime(&midnight(d)),
            When::Utc(t) => t.with_timezone(&rrule::Tz::UTC),
            When::Zoned(n, tz) => resolve(&rrule::Tz::from(*tz), n),
            When::Floating(n) => resolve(&rrule::Tz::LOCAL, n),
        }
    }
}

#[derive(Default, Debug)]
struct Event {
    uid: String,
    summary: String,
    start: Option<When>,
    end: Option<When>,
    duration: Option<Duration>,
    rrule: Option<String>,
    exdates: Vec<When>,
    rdates: Vec<When>,
    /// Set on an edited copy of one occurrence of a repeating event.
    recurrence_id: Option<When>,
    cancelled: bool,
    location: Option<String>,
    conference: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Default)]
pub struct Calendar {
    /// The calendar's own name, if the file carries one.
    pub name: Option<String>,
    events: Vec<Event>,
}

impl Calendar {
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

/// One thing on the agenda.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub id: String,
    pub title: String,
    pub all_day: bool,
    /// Timed: the instant, in UTC. All-day: the date, YYYY-MM-DD.
    pub start: String,
    /// Timed: the instant, in UTC. All-day: the day after the last day.
    pub end: String,
    pub location: Option<String>,
    /// A meeting link (https only), when the event has one.
    pub link: Option<String>,
    pub recurring: bool,
}

// ---- reading the file --------------------------------------------------------------

/// Long lines are wrapped in the file: a line starting with a space or tab
/// continues the previous one.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    lines
}

/// "NAME;PARAM=value:content" -> (NAME, [(PARAM, value)], content)
fn parse_line(line: &str) -> Option<(String, Vec<(String, String)>, String)> {
    let mut in_quotes = false;
    let mut colon = None;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    in_quotes = false;
    for ch in head.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ';' if !in_quotes => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    let name = parts.first()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = parts[1..]
        .iter()
        .filter_map(|p| p.split_once('=').map(|(k, v)| (k.trim().to_ascii_uppercase(), v.trim().to_string())))
        .collect();
    Some((name, params, value.to_string()))
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn one_line(value: &str) -> String {
    unescape(value).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_when(value: &str, params: &[(String, String)]) -> Option<When> {
    let v = value.trim();
    let date_only = params.iter().any(|(k, val)| k == "VALUE" && val.eq_ignore_ascii_case("DATE"))
        || (v.len() == 8 && v.bytes().all(|b| b.is_ascii_digit()));
    if date_only {
        return NaiveDate::parse_from_str(v.get(..8)?, "%Y%m%d").ok().map(When::Date);
    }
    let (body, utc) = match v.strip_suffix(['Z', 'z']) {
        Some(b) => (b, true),
        None => (v, false),
    };
    let naive = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S").ok()?;
    if utc {
        return Some(When::Utc(Utc.from_utc_datetime(&naive)));
    }
    let zone = params.iter().find(|(k, _)| k == "TZID").and_then(|(_, id)| chrono_tz::Tz::from_str(id).ok());
    Some(match zone {
        Some(tz) => When::Zoned(naive, tz),
        None => When::Floating(naive),
    })
}

/// "PT1H30M", "P1D", "P2W" ...
fn parse_duration(value: &str) -> Option<Duration> {
    let v = value.trim().trim_start_matches('+');
    let v = v.strip_prefix('P')?;
    let mut total = Duration::zero();
    let mut number = String::new();
    let mut in_time = false;
    for ch in v.chars() {
        match ch {
            'T' => in_time = true,
            '0'..='9' => number.push(ch),
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                total += match (unit, in_time) {
                    ('W', false) => Duration::weeks(n),
                    ('D', false) => Duration::days(n),
                    ('H', true) => Duration::hours(n),
                    ('M', true) => Duration::minutes(n),
                    ('S', true) => Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    Some(total)
}

fn apply(ev: &mut Event, key: &str, params: &[(String, String)], value: &str) {
    match key {
        "UID" => ev.uid = value.trim().to_string(),
        "SUMMARY" => ev.summary = one_line(value),
        "DTSTART" => ev.start = parse_when(value, params),
        "DTEND" => ev.end = parse_when(value, params),
        "DURATION" => ev.duration = parse_duration(value),
        "RRULE" => ev.rrule = Some(value.trim().to_string()),
        "EXDATE" => ev.exdates.extend(value.split(',').filter_map(|v| parse_when(v, params))),
        "RDATE" => ev.rdates.extend(value.split(',').filter_map(|v| parse_when(v, params))),
        "RECURRENCE-ID" => ev.recurrence_id = parse_when(value, params),
        "STATUS" => ev.cancelled = value.trim().eq_ignore_ascii_case("CANCELLED"),
        "LOCATION" => ev.location = Some(one_line(value)).filter(|s| !s.is_empty()),
        "X-GOOGLE-CONFERENCE" => ev.conference = Some(value.trim().to_string()),
        "DESCRIPTION" => ev.description = Some(unescape(value)),
        _ => {}
    }
}

/// Reads an iCalendar file.
pub fn parse(text: &str) -> Result<Calendar> {
    let text = text.trim_start_matches('\u{feff}');
    let lines = unfold(text);
    if !lines.iter().any(|l| l.trim().eq_ignore_ascii_case("BEGIN:VCALENDAR")) {
        return Err(Error::invalid(
            "That address did not return a calendar. In Google Calendar, copy the \"Secret address in iCal format\" from the calendar's settings.",
        ));
    }
    let mut cal = Calendar::default();
    // Events can contain reminders (VALARM) with their own fields: only fields
    // directly inside the event belong to it, hence the stack.
    let mut stack: Vec<String> = Vec::new();
    let mut current: Option<Event> = None;
    for line in &lines {
        let Some((key, params, value)) = parse_line(line) else { continue };
        match key.as_str() {
            "BEGIN" => {
                let kind = value.trim().to_ascii_uppercase();
                if kind == "VEVENT" && current.is_none() {
                    current = Some(Event::default());
                }
                stack.push(kind);
            }
            "END" => {
                let kind = value.trim().to_ascii_uppercase();
                if kind == "VEVENT" && stack.last().map(String::as_str) == Some("VEVENT") {
                    if let Some(ev) = current.take() {
                        if ev.start.is_some() {
                            cal.events.push(ev);
                        }
                    }
                }
                stack.pop();
            }
            _ => match stack.last().map(String::as_str) {
                Some("VCALENDAR") if key == "X-WR-CALNAME" => cal.name = Some(one_line(&value)).filter(|s| !s.is_empty()),
                Some("VEVENT") => {
                    if let Some(ev) = current.as_mut() {
                        apply(ev, &key, &params, &value);
                    }
                }
                _ => {}
            },
        }
    }
    Ok(cal)
}

// ---- which events fall in a window ----------------------------------------------------

/// `UNTIL` has to be written the way the start is (the recurrence library
/// checks this). Real files are sloppier, so bring it in line.
fn fix_until(rule: &str, local_start: bool) -> String {
    rule.split(';')
        .map(|part| {
            let Some(value) = part.strip_prefix("UNTIL=").or_else(|| part.strip_prefix("until=")) else { return part.to_string() };
            let v = value.trim();
            if v.len() == 8 {
                // A bare date: the whole of that day.
                return format!("UNTIL={v}T235959{}", if local_start { "" } else { "Z" });
            }
            if !local_start && !v.ends_with(['Z', 'z']) {
                return format!("UNTIL={v}Z");
            }
            part.to_string()
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Start instants of a repeating event between `after` and `before`.
fn expand(ev: &Event, start: &When, rule: &str, after: DateTime<Utc>, before: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    let first = start.for_rule();
    let local_start = matches!(start, When::Floating(_));
    let text = format!("RRULE:{}", fix_until(rule, local_start));
    // A rule we cannot read still shows the event on its first date.
    let Ok(parsed) = text.parse::<rrule::RRule<rrule::Unvalidated>>() else { return vec![start.instant()] };
    let Ok(mut set) = parsed.build(first) else { return vec![start.instant()] };
    for ex in &ev.exdates {
        set = set.exdate(ex.for_rule());
    }
    for extra in &ev.rdates {
        set = set.rdate(extra.for_rule());
    }
    set.after(after.with_timezone(&rrule::Tz::UTC))
        .before(before.with_timezone(&rrule::Tz::UTC))
        .all(MAX_INSTANCES)
        .dates
        .into_iter()
        .map(|d| d.with_timezone(&Utc))
        .collect()
}

fn meeting_link(ev: &Event) -> Option<String> {
    let https = |s: &str| s.starts_with("https://") && !s.chars().any(|c| c.is_whitespace() || c == '"');
    if let Some(c) = ev.conference.as_deref().filter(|c| https(c)) {
        return Some(c.to_string());
    }
    if let Some(l) = ev.location.as_deref().filter(|l| https(l)) {
        return Some(l.to_string());
    }
    // Otherwise the first link to a known meeting service in the description.
    let text = ev.description.as_deref()?;
    const HOSTS: [&str; 4] = ["https://meet.google.com/", "https://zoom.us/", "https://teams.microsoft.com/", "https://teams.live.com/"];
    let mut found: Option<(usize, &str)> = None;
    for host in HOSTS {
        if let Some(i) = text.find(host) {
            if found.is_none_or(|(at, _)| i < at) {
                found = Some((i, host));
            }
        }
    }
    let (i, _) = found?;
    let tail = &text[i..];
    let end = tail.find(|c: char| c.is_whitespace() || matches!(c, '"' | '<' | '>' | ')')).unwrap_or(tail.len());
    Some(tail[..end].to_string())
}

/// The events of `cal` that touch the window [`from`, `to`).
pub fn occurrences(cal: &Calendar, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Occurrence> {
    // All-day events are compared by calendar date on this computer.
    let from_date = from.with_timezone(&Local).date_naive();
    let to_local = to.with_timezone(&Local);
    let to_date = if to_local.time() == NaiveTime::MIN { to_local.date_naive() } else { to_local.date_naive() + Duration::days(1) };
    occurrences_in(cal, from, to, from_date, to_date)
}

fn occurrences_in(cal: &Calendar, from: DateTime<Utc>, to: DateTime<Utc>, from_date: NaiveDate, to_date: NaiveDate) -> Vec<Occurrence> {
    // Occurrences that were edited on their own exist as separate events;
    // the repeating event must not also produce them.
    let replaced: HashSet<(&str, i64)> = cal
        .events
        .iter()
        .filter_map(|e| e.recurrence_id.as_ref().map(|r| (e.uid.as_str(), r.instant().timestamp())))
        .collect();

    let mut out: Vec<Occurrence> = Vec::new();
    for ev in &cal.events {
        if ev.cancelled {
            continue;
        }
        let Some(start) = &ev.start else { continue };
        let all_day = start.is_date();
        let length = match (&ev.end, ev.duration) {
            (Some(end), _) => end.instant() - start.instant(),
            (None, Some(d)) => d,
            (None, None) if all_day => Duration::days(1),
            (None, None) => Duration::zero(),
        };
        let length = if all_day { length.max(Duration::days(1)) } else { length.max(Duration::zero()) };

        let starts: Vec<DateTime<Utc>> = match (&ev.rrule, &ev.recurrence_id) {
            (Some(rule), None) => {
                let margin = length + Duration::days(2);
                expand(ev, start, rule, from - margin, to + Duration::days(2))
                    .into_iter()
                    .filter(|s| !replaced.contains(&(ev.uid.as_str(), s.timestamp())))
                    .collect()
            }
            _ => vec![start.instant()],
        };
        let recurring = ev.rrule.is_some() || ev.recurrence_id.is_some();
        let title = if ev.summary.is_empty() { "(No title)".to_string() } else { ev.summary.clone() };

        for s in starts {
            let e = s + length;
            let (start_text, end_text) = if all_day {
                let (first, after_last) = (s.date_naive(), e.date_naive());
                if first >= to_date || after_last <= from_date {
                    continue;
                }
                (first.format("%Y-%m-%d").to_string(), after_last.format("%Y-%m-%d").to_string())
            } else {
                // Skip what starts after the window or ended before it (a
                // zero-length event exactly at the window's start still counts).
                if s >= to || (e <= from && !(s == e && s >= from)) {
                    continue;
                }
                (s.to_rfc3339_opts(chrono::SecondsFormat::Secs, true), e.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            };
            out.push(Occurrence {
                id: format!("{}@{}", ev.uid, s.timestamp()),
                title: title.clone(),
                all_day,
                start: start_text,
                end: end_text,
                location: ev.location.clone().filter(|l| !l.starts_with("https://")),
                link: meeting_link(ev),
                recurring,
            });
        }
    }
    out.sort_by(|a, b| a.start[..10].cmp(&b.start[..10]).then(b.all_day.cmp(&a.all_day)).then(a.start.cmp(&b.start)).then(a.title.cmp(&b.title)));
    out
}

// ---- downloading -----------------------------------------------------------------------

/// Accepts https links, and webcal links (the same thing under another name).
pub fn normalize_url(input: &str) -> Result<String> {
    let s = input.trim();
    let s = match s.strip_prefix("webcal://").or_else(|| s.strip_prefix("webcals://")) {
        Some(rest) => format!("https://{rest}"),
        None => s.to_string(),
    };
    let ok = s.starts_with("https://") && s.len() > 12 && s.len() < 2000 && !s.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"');
    if !ok {
        return Err(Error::invalid(
            "That is not a calendar address. It should start with https:// (in Google Calendar: Settings, your calendar, Integrate calendar, \"Secret address in iCal format\").",
        ));
    }
    Ok(s)
}

/// Downloads the calendar file. No error from here ever contains the address.
pub async fn download(url: &str) -> Result<String> {
    let url = normalize_url(url)?;
    let fail = |why: String| Error::Other(format!("Could not download the calendar: {why}"));
    let http = reqwest::Client::builder()
        .user_agent(concat!("headway-app/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(StdDuration::from_secs(10))
        .timeout(StdDuration::from_secs(30))
        .build()
        .map_err(|e| fail(e.without_url().to_string()))?;
    let resp = http
        .get(&url)
        .header(reqwest::header::ACCEPT, "text/calendar, text/plain;q=0.8, */*;q=0.1")
        .send()
        .await
        // i18n
        .map_err(|e| fail(if e.is_timeout() { "it took too long".into() } else if e.is_connect() { "no connection".into() } else { e.without_url().to_string() }))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::invalid(match status.as_u16() {
            // i18n
            404 => "That calendar address no longer exists. Copy it again from the calendar's settings (it changes when it is reset).".to_string(),
            // i18n
            401 | 403 => "That address may not be read. Use the secret address, not the public one.".to_string(),
            // i18n
            code => format!("The calendar address answered with an error ({code})."),
        }));
    }
    if resp.content_length().is_some_and(|n| n as usize > MAX_BYTES) {
        return Err(Error::invalid("That calendar file is too large to read."));
    }
    let bytes = resp.bytes().await.map_err(|e| fail(e.without_url().to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::invalid("That calendar file is too large to read."));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nPRODID:-//Google Inc//Google Calendar 70.9054//EN\r\nVERSION:2.0\r\nX-WR-CALNAME:Team calendar\r\nX-WR-TIMEZONE:America/Argentina/Buenos_Aires\r\nBEGIN:VTIMEZONE\r\nTZID:America/Argentina/Buenos_Aires\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:-0300\r\nTZOFFSETTO:-0300\r\nDTSTART:19700101T000000\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nDTSTART;TZID=America/Argentina/Buenos_Aires:20261102T093000\r\nDTEND;TZID=America/Argentina/Buenos_Aires:20261102T094500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR\r\nEXDATE;TZID=America/Argentina/Buenos_Aires:20261125T093000\r\nUID:standup@example\r\nSUMMARY:Daily standup\r\nLOCATION:https://meet.google.com/abc-defg-hij\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:This is an event reminder\r\nTRIGGER:-P0DT0H10M0S\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;TZID=America/Argentina/Buenos_Aires:20261127T110000\r\nDTEND;TZID=America/Argentina/Buenos_Aires:20261127T111500\r\nRECURRENCE-ID;TZID=America/Argentina/Buenos_Aires:20261127T093000\r\nUID:standup@example\r\nSUMMARY:Daily standup (moved)\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261126\r\nDTEND;VALUE=DATE:20261127\r\nUID:dayoff@example\r\nSUMMARY:Company day off\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20261126T180000Z\r\nDTEND:20261126T190000Z\r\nUID:review@example\r\nSUMMARY:Sprint review\\, demo\r\n  and retro\r\nLOCATION:Room 2\\; second floor\r\nX-GOOGLE-CONFERENCE:https://meet.google.com/xyz-uvwx-rst\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20261126T200000Z\r\nDTEND:20261126T203000Z\r\nUID:gone@example\r\nSUMMARY:Cancelled call\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20200101\r\nRRULE:FREQ=YEARLY\r\nUID:newyear@example\r\nSUMMARY:New year\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261103\r\nDTEND;VALUE=DATE:20261104\r\nRRULE:FREQ=WEEKLY;UNTIL=20261124;BYDAY=TU\r\nUID:planning@example\r\nSUMMARY:Planning day\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// The week of Monday 23 to Sunday 29 November 2026.
    fn week(cal: &Calendar) -> Vec<Occurrence> {
        occurrences_in(cal, utc(2026, 11, 23, 0, 0), utc(2026, 11, 30, 0, 0), date(2026, 11, 23), date(2026, 11, 30))
    }

    #[test]
    fn reads_a_google_style_calendar() {
        let cal = parse(SAMPLE).unwrap();
        assert_eq!(cal.name.as_deref(), Some("Team calendar"));
        assert_eq!(cal.event_count(), 7);
        let got: Vec<(String, String, bool)> = week(&cal).into_iter().map(|o| (o.start, o.title, o.all_day)).collect();
        assert_eq!(
            got,
            vec![
                // Monday's standup: 09:30 in Buenos Aires is 12:30 UTC.
                ("2026-11-23T12:30:00Z".to_string(), "Daily standup".to_string(), false),
                // Tuesday 24 is the last Planning day (UNTIL is that date, inclusive).
                ("2026-11-24".to_string(), "Planning day".to_string(), true),
                // Wednesday's standup was deleted (EXDATE); nothing on the 25th.
                ("2026-11-26".to_string(), "Company day off".to_string(), true),
                ("2026-11-26T18:00:00Z".to_string(), "Sprint review, demo and retro".to_string(), false),
                // Friday's standup was moved to 11:00 (an edited occurrence replaces the regular one).
                ("2026-11-27T14:00:00Z".to_string(), "Daily standup (moved)".to_string(), false),
            ]
        );
    }

    #[test]
    fn carries_length_place_and_meeting_link() {
        let cal = parse(SAMPLE).unwrap();
        let all = week(&cal);
        let standup = all.iter().find(|o| o.title == "Daily standup").unwrap();
        assert_eq!(standup.end, "2026-11-23T12:45:00Z");
        assert_eq!(standup.link.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
        assert_eq!(standup.location, None); // the place was only a link
        assert!(standup.recurring);
        let review = all.iter().find(|o| o.title.starts_with("Sprint review")).unwrap();
        assert_eq!(review.location.as_deref(), Some("Room 2; second floor"));
        assert_eq!(review.link.as_deref(), Some("https://meet.google.com/xyz-uvwx-rst"));
        assert!(!review.recurring);
        let day_off = all.iter().find(|o| o.title == "Company day off").unwrap();
        assert_eq!((day_off.start.as_str(), day_off.end.as_str()), ("2026-11-26", "2026-11-27"));
    }

    #[test]
    fn a_meeting_keeps_its_clock_time_across_daylight_saving() {
        // New York leaves daylight saving on 1 November 2026: 09:00 is 13:00 UTC before, 14:00 UTC after.
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART;TZID=America/New_York:20261005T090000\nDTEND;TZID=America/New_York:20261005T093000\nRRULE:FREQ=WEEKLY;UNTIL=20261231T235959Z\nUID:w@example\nSUMMARY:Weekly\nEND:VEVENT\nEND:VCALENDAR\n";
        let cal = parse(text).unwrap();
        let got: Vec<String> = occurrences_in(&cal, utc(2026, 10, 25, 0, 0), utc(2026, 11, 8, 0, 0), date(2026, 10, 25), date(2026, 11, 8)).into_iter().map(|o| o.start).collect();
        assert_eq!(got, ["2026-10-26T13:00:00Z", "2026-11-02T14:00:00Z"]);
    }

    #[test]
    fn events_outside_the_window_are_left_out() {
        let cal = parse(SAMPLE).unwrap();
        let later = occurrences_in(&cal, utc(2027, 1, 1, 0, 0), utc(2027, 1, 2, 0, 0), date(2027, 1, 1), date(2027, 1, 2));
        let titles: Vec<&str> = later.iter().map(|o| o.title.as_str()).collect();
        // 1 January 2027 is a Friday: the weekly standup and the yearly all-day event.
        assert_eq!(titles, ["New year", "Daily standup"]);
    }

    #[test]
    fn a_multi_day_event_shows_while_it_lasts() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART;VALUE=DATE:20261120\nDTEND;VALUE=DATE:20261125\nUID:trip@example\nSUMMARY:Offsite\nEND:VEVENT\nEND:VCALENDAR\n";
        let cal = parse(text).unwrap();
        assert_eq!(week(&cal).len(), 1); // 20..24 November overlaps the week from the 23rd
        let after = occurrences_in(&cal, utc(2026, 11, 25, 0, 0), utc(2026, 11, 30, 0, 0), date(2026, 11, 25), date(2026, 11, 30));
        assert!(after.is_empty()); // the 25th is the day after the last day
    }

    #[test]
    fn rejects_what_is_not_a_calendar() {
        assert_eq!(parse("<html><body>Sign in</body></html>").unwrap_err().code(), "invalid");
        assert_eq!(parse("").unwrap_err().code(), "invalid");
    }

    #[test]
    fn only_secure_links_are_accepted() {
        assert_eq!(normalize_url(" https://calendar.google.com/calendar/ical/x/private-y/basic.ics ").unwrap(), "https://calendar.google.com/calendar/ical/x/private-y/basic.ics");
        assert_eq!(normalize_url("webcal://example.com/a.ics").unwrap(), "https://example.com/a.ics");
        assert!(normalize_url("http://example.com/a.ics").is_err());
        assert!(normalize_url("file:///c:/secret.ics").is_err());
        assert!(normalize_url("https://exa mple.com/a.ics").is_err());
        assert!(normalize_url("").is_err());
    }

    #[test]
    fn reads_lines_durations_and_until() {
        assert_eq!(unfold("SUMMARY:one\r\n two\r\nNEXT:x"), ["SUMMARY:onetwo", "NEXT:x"]);
        let (name, params, value) = parse_line("DTSTART;TZID=\"Europe/Berlin\";VALUE=DATE-TIME:20260101T100000").unwrap();
        assert_eq!((name.as_str(), value.as_str()), ("DTSTART", "20260101T100000"));
        assert_eq!(params, vec![("TZID".to_string(), "Europe/Berlin".to_string()), ("VALUE".to_string(), "DATE-TIME".to_string())]);
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("P1D"), Some(Duration::days(1)));
        assert_eq!(parse_duration("P2W"), Some(Duration::weeks(2)));
        assert_eq!(parse_duration("soon"), None);
        assert_eq!(fix_until("FREQ=WEEKLY;UNTIL=20261124;BYDAY=TU", false), "FREQ=WEEKLY;UNTIL=20261124T235959Z;BYDAY=TU");
        assert_eq!(fix_until("FREQ=DAILY;UNTIL=20261124T100000", false), "FREQ=DAILY;UNTIL=20261124T100000Z");
        assert_eq!(fix_until("FREQ=DAILY;UNTIL=20261124T100000Z", false), "FREQ=DAILY;UNTIL=20261124T100000Z");
        assert_eq!(fix_until("FREQ=DAILY;COUNT=3", true), "FREQ=DAILY;COUNT=3");
    }

    #[test]
    fn finds_a_meeting_link_in_the_description() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20261126T100000Z\nDTEND:20261126T103000Z\nUID:z@example\nSUMMARY:Call\nDESCRIPTION:Agenda here.\\nJoin: https://zoom.us/j/123456?pwd=abc and bring notes\nEND:VEVENT\nEND:VCALENDAR\n";
        let cal = parse(text).unwrap();
        assert_eq!(week(&cal)[0].link.as_deref(), Some("https://zoom.us/j/123456?pwd=abc"));
    }
}
