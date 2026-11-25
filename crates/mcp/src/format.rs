//! Plain-text renderings of the core's JSON, written for a model to read:
//! one fact per line, no JSON noise.

use chrono::{Local, NaiveDate};
use serde_json::Value;

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn n(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn is_open(status: &str) -> bool {
    matches!(status, "todo" | "in_progress" | "in_review")
}

/// " (in 3 days)", " (overdue by 2 days)" ... for a due date, relative to `today`.
fn due_hint(due: &str, today: NaiveDate) -> String {
    let Ok(d) = NaiveDate::parse_from_str(due, "%Y-%m-%d") else { return String::new() };
    match (d - today).num_days() {
        0 => " (today)".into(),
        1 => " (tomorrow)".into(),
        -1 => " (overdue by 1 day)".into(),
        x if x < 0 => format!(" (overdue by {} days)", -x),
        x => format!(" (in {x} days)"),
    }
}

fn day(iso: &str) -> &str {
    iso.get(..10).unwrap_or(iso)
}

fn people(list: &[Value]) -> String {
    if list.is_empty() {
        return "unassigned".into();
    }
    list.iter().map(|u| format!("@{}", s(u, "login"))).collect::<Vec<_>>().join(", ")
}

fn pull_ref(p: &Value) -> String {
    let draft = if p.get("isDraft").and_then(Value::as_bool).unwrap_or(false) { ", draft" } else { "" };
    format!("PR #{} ({}{draft})", n(p, "number"), s(p, "state"))
}

fn task_line_on(t: &Value, today: NaiveDate) -> String {
    let status = s(t, "status");
    let mut bits: Vec<String> = Vec::new();
    if let Some(p) = t.get("priority").and_then(Value::as_str) {
        bits.push(p.to_string());
    }
    if let Some(d) = t.get("due").and_then(Value::as_str) {
        let hint = if is_open(status) { due_hint(d, today) } else { String::new() };
        bits.push(format!("due {d}{hint}"));
    }
    bits.push(people(arr(t, "assignees")));
    bits.extend(arr(t, "pulls").iter().map(pull_ref));
    format!("#{} [{status}] {} — {}", n(t, "number"), bits.join(" · "), s(t, "title"))
}

fn status_rank(status: &str) -> usize {
    ["in_progress", "in_review", "todo", "done", "canceled"].iter().position(|x| *x == status).unwrap_or(9)
}

fn sorted_tasks(tasks: &[Value]) -> Vec<&Value> {
    let mut list: Vec<&Value> = tasks.iter().collect();
    list.sort_by(|a, b| {
        status_rank(s(a, "status"))
            .cmp(&status_rank(s(b, "status")))
            // Dated tasks first, earliest first; "~" sorts after any date.
            .then_with(|| a.get("due").and_then(Value::as_str).unwrap_or("~").cmp(b.get("due").and_then(Value::as_str).unwrap_or("~")))
            .then_with(|| n(a, "number").cmp(&n(b, "number")))
    });
    list
}

pub fn projects(v: &Value) -> String {
    let list = arr(v, "projects");
    if list.is_empty() {
        return "No project is set up in Headway yet. Open the Headway app and add a repository.".into();
    }
    let current = s(v, "currentId");
    let mut out = format!("{} set up in Headway:\n", plural(list.len(), "project", "projects"));
    for p in list {
        let id = s(p, "id");
        let mark = if id == current { " (current)" } else { "" };
        match p.get("localPath").and_then(Value::as_str) {
            Some(path) => out.push_str(&format!("- {id}{mark}, local folder: {path}\n")),
            None => out.push_str(&format!("- {id}{mark}, no local folder set\n")),
        }
    }
    out.trim_end().to_string()
}

pub fn task_list(v: &Value) -> String {
    task_list_on(v, Local::now().date_naive())
}

fn task_list_on(v: &Value, today: NaiveDate) -> String {
    let tasks = arr(v, "tasks");
    if tasks.is_empty() {
        return format!("No matching tasks in {}.", s(v, "project"));
    }
    let mut out = format!("{}: {}\n", s(v, "project"), plural(tasks.len(), "task", "tasks"));
    for t in sorted_tasks(tasks) {
        out.push_str(&task_line_on(t, today));
        out.push('\n');
    }
    if v.get("truncated").and_then(Value::as_bool).unwrap_or(false) {
        out.push_str("(This repository has more open issues than Headway loads; the list is incomplete.)\n");
    }
    out.trim_end().to_string()
}

pub fn task_detail(t: &Value) -> String {
    task_detail_on(t, Local::now().date_naive())
}

fn task_detail_on(t: &Value, today: NaiveDate) -> String {
    let status = s(t, "status");
    let mut out = format!("#{} {}\n", n(t, "number"), s(t, "title"));
    let mut line = format!("Status: {status}");
    line.push_str(&format!(" · Priority: {}", t.get("priority").and_then(Value::as_str).unwrap_or("none")));
    match t.get("due").and_then(Value::as_str) {
        Some(d) => line.push_str(&format!(" · Due: {d}{}", if is_open(status) { due_hint(d, today) } else { String::new() })),
        None => line.push_str(" · Due: none"),
    }
    out.push_str(&line);
    out.push('\n');
    out.push_str(&format!("Assignees: {}", people(arr(t, "assignees"))));
    if let Some(a) = t.get("author").filter(|a| a.is_object()) {
        out.push_str(&format!(" · Author: @{}", s(a, "login")));
    }
    out.push_str(&format!(" · Created {} · Updated {}\n", day(s(t, "createdAt")), day(s(t, "updatedAt"))));
    for p in arr(t, "pulls") {
        out.push_str(&format!("Pull request: #{} ({}{}) {} {}\n", n(p, "number"), s(p, "state"), if p.get("isDraft").and_then(Value::as_bool).unwrap_or(false) { ", draft" } else { "" }, s(p, "title"), s(p, "url")));
    }
    let branches: Vec<&str> = arr(t, "branches").iter().filter_map(Value::as_str).collect();
    if !branches.is_empty() {
        out.push_str(&format!("Linked branches: {}\n", branches.join(", ")));
    }
    let labels: Vec<&str> = arr(t, "labels").iter().map(|l| s(l, "name")).collect();
    if !labels.is_empty() {
        out.push_str(&format!("Labels: {}\n", labels.join(", ")));
    }
    out.push_str(&format!("URL: {}\n", s(t, "url")));

    let body = s(t, "body").trim();
    out.push_str("\nDescription:\n");
    out.push_str(if body.is_empty() { "(none)" } else { body });
    out.push('\n');

    if let Some(comments) = t.get("comments").and_then(Value::as_array) {
        out.push_str(&format!("\nComments ({}):\n", comments.len()));
        for c in comments {
            let who = c.get("author").filter(|a| a.is_object()).map(|a| s(a, "login")).unwrap_or("ghost");
            let when = s(c, "createdAt").replace('T', " ").replace('Z', " UTC");
            out.push_str(&format!("@{who}, {when}:\n{}\n\n", s(c, "body").trim()));
        }
    } else if n(t, "commentCount") > 0 {
        out.push_str(&format!("\n{} (use get_task to read them)\n", plural(n(t, "commentCount") as usize, "comment", "comments")));
    }
    out.trim_end().to_string()
}

pub fn members(snap: &Value) -> String {
    let members = arr(snap, "members");
    let tasks = arr(snap, "tasks");
    let pulls = arr(snap, "pulls");
    let mut out = format!("{}: {}\n", s(snap, "projectId"), plural(members.len(), "member", "members"));
    for m in members {
        let login = s(m, "login");
        let mine: Vec<&Value> = tasks
            .iter()
            .filter(|t| is_open(s(t, "status")) && arr(t, "assignees").iter().any(|a| s(a, "login").eq_ignore_ascii_case(login)))
            .collect();
        let doing: Vec<String> = mine
            .iter()
            .filter(|t| s(t, "status") != "todo")
            .map(|t| format!("#{} {} [{}]", n(t, "number"), s(t, "title"), s(t, "status")))
            .collect();
        let open_prs = pulls
            .iter()
            .filter(|p| s(p, "state") == "open" && p.get("author").map(|a| s(a, "login").eq_ignore_ascii_case(login)).unwrap_or(false))
            .count();
        let name = m.get("name").and_then(Value::as_str).filter(|x| !x.eq_ignore_ascii_case(login)).map(|x| format!(" ({x})")).unwrap_or_default();
        out.push_str(&format!(
            "- @{login}{name}, {}: {}, {}\n",
            s(m, "role"),
            plural(mine.len(), "open task", "open tasks"),
            plural(open_prs, "open pull request", "open pull requests")
        ));
        for d in doing {
            out.push_str(&format!("    working on {d}\n"));
        }
    }
    let unassigned = tasks.iter().filter(|t| is_open(s(t, "status")) && arr(t, "assignees").is_empty()).count();
    if unassigned > 0 {
        out.push_str(&format!("{} unassigned.\n", plural(unassigned, "open task is", "open tasks are")));
    }
    out.trim_end().to_string()
}

fn pull_line(p: &Value) -> String {
    let mut state = s(p, "state").to_string();
    if p.get("isDraft").and_then(Value::as_bool).unwrap_or(false) {
        state.push_str(", draft");
    }
    let mut bits: Vec<String> = Vec::new();
    if let Some(a) = p.get("author").filter(|a| a.is_object()) {
        bits.push(format!("@{}", s(a, "login")));
    }
    bits.push(format!("{} -> {}", s(p, "headRef"), s(p, "baseRef")));
    let closes: Vec<String> = arr(p, "tasks").iter().filter_map(Value::as_u64).map(|x| format!("#{x}")).collect();
    bits.push(if closes.is_empty() { "no linked task".into() } else { format!("closes {}", closes.join(", ")) });
    if let Some(c) = p.get("checks").filter(|c| c.is_object()) {
        bits.push(format!("checks {}/{} passed ({})", n(c, "passed"), n(c, "total"), s(c, "state")));
    }
    if let Some(r) = p.get("review").and_then(Value::as_str) {
        bits.push(format!("review: {}", r.replace('_', " ")));
    }
    bits.push(format!("updated {}", day(s(p, "updatedAt"))));
    format!("PR #{} [{state}] {} — {}", n(p, "number"), s(p, "title"), bits.join(" · "))
}

pub fn pulls(snap: &Value, state: &str) -> String {
    let list: Vec<&Value> = arr(snap, "pulls").iter().filter(|p| state == "all" || s(p, "state") == state).collect();
    if list.is_empty() {
        return format!("No {} pull requests in {}.", if state == "all" { "recent" } else { state }, s(snap, "projectId"));
    }
    let mut out = format!("{}: {}\n", s(snap, "projectId"), plural(list.len(), "pull request", "pull requests"));
    for p in list {
        out.push_str(&pull_line(p));
        out.push('\n');
    }
    out.trim_end().to_string()
}

pub fn status(snap: &Value) -> String {
    status_on(snap, Local::now().date_naive())
}

fn status_on(snap: &Value, today: NaiveDate) -> String {
    let repo = &snap["repo"];
    let tasks = arr(snap, "tasks");
    let mut out = format!("{} ({})", s(repo, "nameWithOwner"), if repo["isPrivate"].as_bool().unwrap_or(false) { "private" } else { "public" });
    if let Some(d) = repo.get("description").and_then(Value::as_str) {
        out.push_str(&format!(": {d}"));
    }
    out.push('\n');
    out.push_str(&format!(
        "Default branch: {} · {} · {}\n",
        repo.get("defaultBranch").and_then(Value::as_str).unwrap_or("(none)"),
        plural(n(repo, "totalCommits") as usize, "commit", "commits"),
        plural(n(repo, "branchCount") as usize, "branch", "branches"),
    ));
    if let Some(c) = repo.get("lastCommit").filter(|c| c.is_object()) {
        let who = c.get("authorLogin").and_then(Value::as_str).map(|l| format!("@{l}")).unwrap_or_else(|| s(c, "authorName").to_string());
        out.push_str(&format!("Last commit: \"{}\" by {who} on {}\n", s(c, "headline"), day(s(c, "committedAt"))));
    }
    out.push_str(&format!("Commits on the default branch in the last 28 days: {}\n", arr(repo, "recentCommits").len()));

    let count = |st: &str| tasks.iter().filter(|t| s(t, "status") == st).count();
    out.push_str(&format!(
        "Tasks: {} open ({} to do, {} in progress, {} in review) · {} closed\n",
        n(repo, "openIssues"),
        count("todo"),
        count("in_progress"),
        count("in_review"),
        n(repo, "closedIssues"),
    ));
    out.push_str(&format!(
        "Pull requests: {} open · {} merged · {} closed without merging\n",
        n(repo, "openPulls"),
        n(repo, "mergedPulls"),
        n(repo, "closedPulls"),
    ));

    let today_text = today.format("%Y-%m-%d").to_string();
    let overdue: Vec<&Value> = sorted_tasks(tasks)
        .into_iter()
        .filter(|t| is_open(s(t, "status")) && t.get("due").and_then(Value::as_str).is_some_and(|d| d < today_text.as_str()))
        .collect();
    if overdue.is_empty() {
        out.push_str("No overdue tasks.\n");
    } else {
        out.push_str(&format!("Overdue ({}):\n", overdue.len()));
        for t in overdue {
            out.push_str(&format!("  {}\n", task_line_on(t, today)));
        }
    }
    let open_pulls: Vec<&Value> = arr(snap, "pulls").iter().filter(|p| s(p, "state") == "open").collect();
    if !open_pulls.is_empty() {
        out.push_str("Open pull requests:\n");
        for p in open_pulls {
            out.push_str(&format!("  {}\n", pull_line(p)));
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()
    }

    #[test]
    fn due_hints_are_relative_to_today() {
        assert_eq!(due_hint("2026-11-25", today()), " (today)");
        assert_eq!(due_hint("2026-11-26", today()), " (tomorrow)");
        assert_eq!(due_hint("2026-11-28", today()), " (in 3 days)");
        assert_eq!(due_hint("2026-11-24", today()), " (overdue by 1 day)");
        assert_eq!(due_hint("2026-11-20", today()), " (overdue by 5 days)");
        assert_eq!(due_hint("soon", today()), "");
    }

    #[test]
    fn task_lines_carry_the_useful_facts() {
        let t = json!({
            "number": 1, "title": "Set up CI", "status": "in_progress", "priority": "high", "due": "2026-11-28",
            "assignees": [{ "login": "ana" }, { "login": "ben" }],
            "pulls": [{ "number": 10, "state": "open", "isDraft": true }]
        });
        assert_eq!(task_line_on(&t, today()), "#1 [in_progress] high · due 2026-11-28 (in 3 days) · @ana, @ben · PR #10 (open, draft) — Set up CI");
        let done = json!({ "number": 5, "title": "Old", "status": "done", "due": "2026-11-01", "assignees": [], "pulls": [] });
        assert_eq!(task_line_on(&done, today()), "#5 [done] due 2026-11-01 · unassigned — Old");
    }

    #[test]
    fn lists_put_active_work_first_then_by_due_date() {
        let v = json!({ "project": "o/r", "tasks": [
            { "number": 3, "title": "c", "status": "todo", "assignees": [], "pulls": [] },
            { "number": 2, "title": "b", "status": "todo", "due": "2026-11-26", "assignees": [], "pulls": [] },
            { "number": 4, "title": "d", "status": "in_review", "assignees": [], "pulls": [] },
            { "number": 1, "title": "a", "status": "in_progress", "assignees": [], "pulls": [] }
        ]});
        let text = task_list_on(&v, today());
        let order: Vec<&str> = text.lines().skip(1).map(|l| l.split(' ').next().unwrap()).collect();
        assert_eq!(order, ["#1", "#4", "#2", "#3"]);
        assert!(text.starts_with("o/r: 4 tasks"));
        assert_eq!(task_list_on(&json!({ "project": "o/r", "tasks": [] }), today()), "No matching tasks in o/r.");
    }

    #[test]
    fn detail_shows_description_and_comments() {
        let t = json!({
            "number": 7, "title": "Fix save", "status": "todo", "priority": null, "due": "2026-11-24",
            "assignees": [], "author": { "login": "ana" }, "createdAt": "2026-11-20T10:00:00Z", "updatedAt": "2026-11-21T09:00:00Z",
            "pulls": [], "branches": ["7-fix-save"], "labels": [{ "name": "bug" }], "url": "https://github.com/o/r/issues/7",
            "body": "Steps to repro.", "comments": [{ "author": { "login": "ben" }, "createdAt": "2026-11-21T09:00:00Z", "body": "On it." }]
        });
        let text = task_detail_on(&t, today());
        assert!(text.contains("Status: todo · Priority: none · Due: 2026-11-24 (overdue by 1 day)"));
        assert!(text.contains("Assignees: unassigned · Author: @ana · Created 2026-11-20 · Updated 2026-11-21"));
        assert!(text.contains("Linked branches: 7-fix-save"));
        assert!(text.contains("Labels: bug"));
        assert!(text.contains("Description:\nSteps to repro."));
        assert!(text.contains("Comments (1):\n@ben, 2026-11-21 09:00:00 UTC:\nOn it."));
    }

    #[test]
    fn status_summarises_the_project() {
        let snap = json!({
            "projectId": "o/r",
            "repo": {
                "nameWithOwner": "o/r", "isPrivate": true, "description": "A game", "defaultBranch": "main", "totalCommits": 7, "branchCount": 4,
                "lastCommit": { "headline": "Add audio", "authorLogin": "ana", "committedAt": "2026-11-25T17:00:00Z" },
                "recentCommits": [{}, {}, {}], "openIssues": 2, "closedIssues": 3, "openPulls": 1, "mergedPulls": 2, "closedPulls": 0
            },
            "tasks": [
                { "number": 7, "title": "Fix save", "status": "todo", "due": "2026-11-24", "assignees": [], "pulls": [] },
                { "number": 1, "title": "CI", "status": "in_progress", "due": "2026-11-28", "assignees": [{ "login": "ana" }], "pulls": [] }
            ],
            "pulls": [{ "number": 10, "title": "CI", "state": "open", "isDraft": true, "author": { "login": "ana" }, "headRef": "1-ci", "baseRef": "main", "tasks": [1], "updatedAt": "2026-11-25T17:00:00Z" }],
            "members": [{ "login": "ana", "name": "Ana P", "role": "admin" }]
        });
        let text = status_on(&snap, today());
        assert!(text.starts_with("o/r (private): A game\nDefault branch: main · 7 commits · 4 branches\n"));
        assert!(text.contains("Last commit: \"Add audio\" by @ana on 2026-11-25"));
        assert!(text.contains("Tasks: 2 open (1 to do, 1 in progress, 0 in review) · 3 closed"));
        assert!(text.contains("Overdue (1):\n  #7 [todo] due 2026-11-24 (overdue by 1 day) · unassigned — Fix save"));
        assert!(text.contains("PR #10 [open, draft] CI — @ana · 1-ci -> main · closes #1 · updated 2026-11-25"));

        let m = members(&snap);
        assert!(m.contains("- @ana (Ana P), admin: 1 open task, 1 open pull request\n    working on #1 CI [in_progress]"));
        assert!(m.contains("1 open task is unassigned."));
        assert_eq!(pulls(&snap, "merged"), "No merged pull requests in o/r.");
    }
}
