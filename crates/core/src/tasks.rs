//! How a Headway task maps onto a GitHub issue.
//!
//! * status   -> open/closed plus one of two labels
//! * priority -> a `priority: ...` label
//! * due date -> a hidden HTML comment at the end of the issue body
//!
//! Everything else (title, description, assignees, comments) is the issue's own.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::model::{Priority, Status};

pub const LABEL_IN_PROGRESS: &str = "status: in progress";
pub const LABEL_IN_REVIEW: &str = "status: in review";

pub struct LabelSpec {
    pub name: &'static str,
    pub color: &'static str,
    pub description: &'static str,
}

/// The labels Headway creates in a repo the first time it needs them.
pub const MANAGED_LABELS: &[LabelSpec] = &[
    LabelSpec { name: LABEL_IN_PROGRESS, color: "0A84FF", description: "Someone is working on this task" },
    LabelSpec { name: LABEL_IN_REVIEW, color: "8B5CF6", description: "The work is waiting for review" },
    LabelSpec { name: "priority: urgent", color: "D92D20", description: "Drop everything" },
    LabelSpec { name: "priority: high", color: "F97316", description: "Do this first" },
    LabelSpec { name: "priority: medium", color: "EAB308", description: "Normal priority" },
    LabelSpec { name: "priority: low", color: "94A3B8", description: "When there is time" },
];

pub fn status_label(s: Status) -> Option<&'static str> {
    match s {
        Status::InProgress => Some(LABEL_IN_PROGRESS),
        Status::InReview => Some(LABEL_IN_REVIEW),
        _ => None,
    }
}

pub fn priority_label(p: Priority) -> &'static str {
    match p {
        Priority::Urgent => "priority: urgent",
        Priority::High => "priority: high",
        Priority::Medium => "priority: medium",
        Priority::Low => "priority: low",
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LabelKind {
    Status(Status),
    Priority(Priority),
    Other,
}

/// "Status: In-Progress" -> "status:in progress"
fn normalize(name: &str) -> String {
    let lower = name.trim().to_lowercase().replace(['-', '_'], " ");
    let mut out = String::with_capacity(lower.len());
    let mut pending_space = false;
    for ch in lower.chars() {
        if ch.is_whitespace() {
            pending_space = true;
        } else if ch == ':' || ch == '/' {
            out.push(ch);
            pending_space = false;
        } else {
            if pending_space && !out.is_empty() && !out.ends_with([':', '/']) {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        }
    }
    out
}

/// Reads our own label names and the common spellings other teams already use.
pub fn classify(name: &str) -> LabelKind {
    let n = normalize(name);
    let (prefix, rest) = match n.split_once([':', '/']) {
        Some((p, r)) => (p, r),
        None => ("", n.as_str()),
    };
    match prefix {
        "status" | "state" => match rest {
            "in progress" | "inprogress" | "doing" | "wip" | "started" => LabelKind::Status(Status::InProgress),
            "in review" | "inreview" | "review" | "needs review" => LabelKind::Status(Status::InReview),
            _ => LabelKind::Other,
        },
        // Also reads numbered spellings such as "priority: 1 high".
        "priority" | "prio" => Priority::parse(rest)
            .or_else(|| rest.split_whitespace().find_map(Priority::parse))
            .map(LabelKind::Priority)
            .unwrap_or(LabelKind::Other),
        "" => match rest {
            "in progress" | "wip" | "doing" => LabelKind::Status(Status::InProgress),
            "in review" | "needs review" => LabelKind::Status(Status::InReview),
            "urgent" | "p0" => LabelKind::Priority(Priority::Urgent),
            "high priority" | "p1" => LabelKind::Priority(Priority::High),
            "p2" => LabelKind::Priority(Priority::Medium),
            "low priority" | "p3" => LabelKind::Priority(Priority::Low),
            _ => LabelKind::Other,
        },
        _ => LabelKind::Other,
    }
}

pub fn derive_status<S: AsRef<str>>(closed: bool, state_reason: Option<&str>, labels: &[S]) -> Status {
    if closed {
        return match state_reason.map(|r| r.to_ascii_uppercase()).as_deref() {
            Some("NOT_PLANNED") | Some("DUPLICATE") => Status::Canceled,
            _ => Status::Done,
        };
    }
    let mut found = Status::Todo;
    for l in labels {
        match classify(l.as_ref()) {
            LabelKind::Status(Status::InReview) => return Status::InReview,
            LabelKind::Status(s) => found = s,
            _ => {}
        }
    }
    found
}

/// When several priority labels are present the most urgent one wins.
pub fn derive_priority<S: AsRef<str>>(labels: &[S]) -> Option<Priority> {
    let rank = |p: Priority| Priority::ALL.iter().position(|x| *x == p).unwrap_or(usize::MAX);
    labels
        .iter()
        .filter_map(|l| match classify(l.as_ref()) {
            LabelKind::Priority(p) => Some(p),
            _ => None,
        })
        .min_by_key(|p| rank(*p))
}

/// The label list to send to GitHub. `status`/`priority` are `None` to leave
/// that aspect untouched; `Some(None)` for priority clears it.
pub fn apply_labels<S: AsRef<str>>(
    current: &[S],
    status: Option<Status>,
    priority: Option<Option<Priority>>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in current {
        let name = l.as_ref();
        let keep = match classify(name) {
            LabelKind::Status(_) => status.is_none(),
            LabelKind::Priority(_) => priority.is_none(),
            LabelKind::Other => true,
        };
        if keep && !out.iter().any(|o| o.eq_ignore_ascii_case(name)) {
            out.push(name.to_string());
        }
    }
    if let Some(name) = status.and_then(status_label) {
        out.push(name.to_string());
    }
    if let Some(Some(p)) = priority {
        out.push(priority_label(p).to_string());
    }
    out
}

static META_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<!--\s*headway:\s*(\{.*?\})\s*-->").expect("valid regex"));

/// Extra task fields GitHub has no place for, kept as JSON inside an HTML
/// comment in the issue body (GitHub does not render HTML comments).
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Meta(pub Map<String, Value>);

impl Meta {
    pub fn due(&self) -> Option<String> {
        self.0.get("due").and_then(Value::as_str).filter(|s| is_date(s)).map(str::to_string)
    }

    pub fn set_due(&mut self, due: Option<&str>) {
        match due {
            Some(d) => {
                self.0.insert("due".into(), Value::String(d.to_string()));
            }
            None => {
                self.0.remove("due");
            }
        }
    }
}

pub fn is_date(s: &str) -> bool {
    s.len() == 10 && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// Splits an issue body into the text people wrote and Headway's metadata.
pub fn split_body(raw: &str) -> (String, Meta) {
    let mut meta = Meta::default();
    let mut body = raw.to_string();
    if let Some(c) = META_RE.captures(raw) {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&c[1]) {
            meta.0 = map;
        }
        let whole = c.get(0).expect("group 0 always exists");
        body.replace_range(whole.range(), "");
    }
    (body.trim_end().to_string(), meta)
}

pub fn join_body(body: &str, meta: &Meta) -> String {
    let body = body.trim_end();
    if meta.0.is_empty() {
        return body.to_string();
    }
    let json = Value::Object(meta.0.clone()).to_string();
    if body.is_empty() {
        format!("<!-- headway: {json} -->")
    } else {
        format!("{body}\n\n<!-- headway: {json} -->")
    }
}

/// A git-safe branch name for a task: "12-fix-login-button".
pub fn branch_name(number: u64, title: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if dash && !slug.is_empty() {
                slug.push('-');
            }
            dash = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            dash = true;
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() { format!("{number}-task") } else { format!("{number}-{slug}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_own_and_common_labels() {
        assert_eq!(classify("status: in progress"), LabelKind::Status(Status::InProgress));
        assert_eq!(classify("Status: In-Progress"), LabelKind::Status(Status::InProgress));
        assert_eq!(classify("status/in_review"), LabelKind::Status(Status::InReview));
        assert_eq!(classify("in progress"), LabelKind::Status(Status::InProgress));
        assert_eq!(classify("priority: high"), LabelKind::Priority(Priority::High));
        assert_eq!(classify("Priority / Urgent"), LabelKind::Priority(Priority::Urgent));
        assert_eq!(classify("P2"), LabelKind::Priority(Priority::Medium));
        assert_eq!(classify("priority: 1 high"), LabelKind::Priority(Priority::High));
        assert_eq!(classify("priority: 0 critical"), LabelKind::Priority(Priority::Urgent));
        assert_eq!(classify("bug"), LabelKind::Other);
        assert_eq!(classify("review"), LabelKind::Other);
        assert_eq!(classify("status: blocked"), LabelKind::Other);
        assert_eq!(classify("priority: someday"), LabelKind::Other);
    }

    #[test]
    fn status_comes_from_state_then_labels() {
        let none: [&str; 0] = [];
        assert_eq!(derive_status(false, None, &none), Status::Todo);
        assert_eq!(derive_status(false, None, &["bug", LABEL_IN_PROGRESS]), Status::InProgress);
        assert_eq!(derive_status(false, None, &[LABEL_IN_PROGRESS, LABEL_IN_REVIEW]), Status::InReview);
        assert_eq!(derive_status(true, Some("COMPLETED"), &[LABEL_IN_PROGRESS]), Status::Done);
        assert_eq!(derive_status(true, Some("not_planned"), &none), Status::Canceled);
        assert_eq!(derive_status(true, None, &none), Status::Done);
    }

    #[test]
    fn most_urgent_priority_wins() {
        assert_eq!(derive_priority(&["priority: low", "priority: urgent"]), Some(Priority::Urgent));
        assert_eq!(derive_priority(&["bug"]), None);
    }

    #[test]
    fn label_updates_keep_unrelated_labels() {
        let cur = ["bug", LABEL_IN_PROGRESS, "priority: low"];
        assert_eq!(
            apply_labels(&cur, Some(Status::InReview), None),
            vec!["bug", "priority: low", LABEL_IN_REVIEW]
        );
        assert_eq!(apply_labels(&cur, Some(Status::Todo), None), vec!["bug", "priority: low"]);
        assert_eq!(apply_labels(&cur, Some(Status::Done), None), vec!["bug", "priority: low"]);
        assert_eq!(
            apply_labels(&cur, None, Some(Some(Priority::High))),
            vec!["bug", LABEL_IN_PROGRESS, "priority: high"]
        );
        assert_eq!(apply_labels(&cur, None, Some(None)), vec!["bug", LABEL_IN_PROGRESS]);
        assert_eq!(apply_labels(&cur, None, None), cur.to_vec());
    }

    #[test]
    fn body_metadata_round_trips() {
        let (body, meta) = split_body("Fix the thing.\r\n\r\n<!-- headway: {\"due\":\"2026-12-01\"} -->\n");
        assert_eq!(body, "Fix the thing.");
        assert_eq!(meta.due().as_deref(), Some("2026-12-01"));
        let joined = join_body(&body, &meta);
        assert_eq!(joined, "Fix the thing.\n\n<!-- headway: {\"due\":\"2026-12-01\"} -->");
        assert_eq!(split_body(&joined), (body, meta));

        let (body, meta) = split_body("No metadata here");
        assert_eq!(body, "No metadata here");
        assert!(meta.0.is_empty());
        assert_eq!(join_body(&body, &meta), "No metadata here");

        // Unknown keys survive an edit of the due date.
        let (body, mut meta) = split_body("x\n<!-- headway: {\"due\":\"2026-01-02\",\"later\":1} -->");
        meta.set_due(None);
        assert_eq!(join_body(&body, &meta), "x\n\n<!-- headway: {\"later\":1} -->");

        // A broken date is ignored instead of shown.
        let (_, meta) = split_body("<!-- headway: {\"due\":\"soon\"} -->");
        assert_eq!(meta.due(), None);
        assert_eq!(join_body("", &Meta::default()), "");
    }

    #[test]
    fn branch_names_are_git_safe() {
        assert_eq!(branch_name(12, "Fix login button!"), "12-fix-login-button");
        assert_eq!(branch_name(3, "  ¿Qué pasó?  "), "3-qu-pas");
        assert_eq!(branch_name(7, "???"), "7-task");
        assert!(branch_name(1, &"very long title ".repeat(10)).len() <= 44);
    }
}
