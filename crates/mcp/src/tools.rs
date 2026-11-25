//! The tools Claude sees, and how each maps onto the core's API.

use std::path::Path;

use headway_core::config::{self, Config};
use headway_core::Service;
use serde_json::{json, Map, Value};

use crate::format;

const PROJECT: &str = "Repository as owner/name. Leave out to use the project of the folder you are working in, or else the one selected in the Headway app.";
const STATUSES: [&str; 5] = ["todo", "in_progress", "in_review", "done", "canceled"];

struct Spec {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    properties: Value,
    required: &'static [&'static str],
    read_only: bool,
    /// Calling it twice with the same arguments changes nothing more.
    idempotent: bool,
}

fn project_prop() -> Value {
    json!({ "type": "string", "description": PROJECT })
}

fn number_prop(what: &str) -> Value {
    json!({ "type": "integer", "minimum": 1, "description": what })
}

fn specs() -> Vec<Spec> {
    let task_fields = |with_none: bool| {
        let mut priorities = vec!["urgent", "high", "medium", "low"];
        if with_none {
            priorities.push("none");
        }
        json!({
            "status": { "type": "string", "enum": STATUSES, "description": "Where the task stands." },
            "priority": { "type": "string", "enum": priorities, "description": if with_none { "Use \"none\" to clear it." } else { "Leave out for no priority." } },
            "due": { "type": "string", "description": if with_none { "Due date as YYYY-MM-DD, or \"none\" to clear it." } else { "Due date as YYYY-MM-DD." } },
            "assignees": { "type": "array", "items": { "type": "string" }, "description": if with_none { "GitHub user names. Replaces the current assignees; an empty list unassigns everyone." } else { "GitHub user names." } },
        })
    };
    let mut create = task_fields(false);
    create["project"] = project_prop();
    create["title"] = json!({ "type": "string", "description": "Short, specific title." });
    create["description"] = json!({ "type": "string", "description": "Markdown description." });
    let mut update = task_fields(true);
    update["project"] = project_prop();
    update["number"] = number_prop("Task number (the GitHub issue number).");
    update["title"] = json!({ "type": "string", "description": "New title." });
    update["description"] = json!({ "type": "string", "description": "New Markdown description. Replaces the current one entirely." });

    vec![
        Spec {
            name: "list_projects",
            title: "List projects",
            description: "List the repositories set up in Headway and which one is current.",
            properties: json!({}),
            required: &[],
            read_only: true,
            idempotent: true,
        },
        Spec {
            name: "list_tasks",
            title: "List tasks",
            description: "List a project's tasks with status, priority, due date, assignees and linked pull requests. Open tasks only unless a status is given or include_done is true.",
            properties: json!({
                "project": project_prop(),
                "status": { "type": "string", "enum": STATUSES, "description": "Only tasks in this status." },
                "assignee": { "type": "string", "description": "A GitHub user name, \"me\", or \"none\" for unassigned tasks." },
                "search": { "type": "string", "description": "Text to look for in titles and descriptions." },
                "include_done": { "type": "boolean", "description": "Also list done and canceled tasks." },
            }),
            required: &[],
            read_only: true,
            idempotent: true,
        },
        Spec {
            name: "get_task",
            title: "Get task",
            description: "Read one task in full: description, comments, linked pull requests and branches.",
            properties: json!({ "project": project_prop(), "number": number_prop("Task number (the GitHub issue number).") }),
            required: &["number"],
            read_only: true,
            idempotent: true,
        },
        Spec {
            name: "create_task",
            title: "Create task",
            description: "Create a task (a GitHub issue) in the project.",
            properties: create,
            required: &["title"],
            read_only: false,
            idempotent: false,
        },
        Spec {
            name: "update_task",
            title: "Update task",
            description: "Change a task's status, priority, due date, assignees, title or description. Only the fields you pass are changed.",
            properties: update,
            required: &["number"],
            read_only: false,
            idempotent: true,
        },
        Spec {
            name: "add_comment",
            title: "Add comment",
            description: "Add a Markdown comment to a task.",
            properties: json!({
                "project": project_prop(),
                "number": number_prop("Task number (the GitHub issue number)."),
                "body": { "type": "string", "description": "The comment, in Markdown." },
            }),
            required: &["number", "body"],
            read_only: false,
            idempotent: false,
        },
        Spec {
            name: "list_members",
            title: "List members",
            description: "List the project's members with their role and what each is working on.",
            properties: json!({ "project": project_prop() }),
            required: &[],
            read_only: true,
            idempotent: true,
        },
        Spec {
            name: "list_pull_requests",
            title: "List pull requests",
            description: "List recent pull requests with their state, checks, review decision and the tasks they close.",
            properties: json!({
                "project": project_prop(),
                "state": { "type": "string", "enum": ["open", "merged", "closed", "all"], "description": "Defaults to open." },
            }),
            required: &[],
            read_only: true,
            idempotent: true,
        },
        Spec {
            name: "link_pull_request",
            title: "Link pull request to task",
            description: "Link a pull request to a task by adding \"Closes #<task>\" to the pull request's description. GitHub then closes the task when the pull request is merged into the default branch.",
            properties: json!({
                "project": project_prop(),
                "pull": number_prop("Pull request number."),
                "task": number_prop("Task number."),
            }),
            required: &["pull", "task"],
            read_only: false,
            idempotent: true,
        },
        Spec {
            name: "start_branch",
            title: "Start a branch for a task",
            description: "Create a branch on GitHub from the default branch, linked to the task. A pull request opened from that branch is attached to the task automatically. Returns the branch name to check out.",
            properties: json!({
                "project": project_prop(),
                "number": number_prop("Task number."),
                "name": { "type": "string", "description": "Branch name. Defaults to <number>-<title-in-kebab-case>." },
            }),
            required: &["number"],
            read_only: false,
            idempotent: false,
        },
        Spec {
            name: "project_status",
            title: "Project status",
            description: "Summarise the project: the repository, the last commit, commit totals, task counts by status, overdue tasks and open pull requests.",
            properties: json!({ "project": project_prop() }),
            required: &[],
            read_only: true,
            idempotent: true,
        },
    ]
}

pub fn definitions() -> Vec<Value> {
    specs()
        .into_iter()
        .map(|s| {
            let mut schema = json!({ "type": "object", "properties": s.properties, "additionalProperties": false });
            if !s.required.is_empty() {
                schema["required"] = json!(s.required);
            }
            json!({
                "name": s.name,
                "title": s.title,
                "description": s.description,
                "inputSchema": schema,
                "annotations": {
                    "title": s.title,
                    "readOnlyHint": s.read_only,
                    "destructiveHint": false,
                    "idempotentHint": s.idempotent,
                    "openWorldHint": true,
                },
            })
        })
        .collect()
}

pub fn exists(name: &str) -> bool {
    specs().iter().any(|s| s.name == name)
}

fn done(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}

fn failed(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// Runs a tool. Problems (bad arguments, GitHub errors) come back as a tool
/// error the model can read and react to, never as a protocol error.
pub async fn call(service: &Service, name: &str, args: Value) -> Value {
    let args = match args {
        Value::Object(m) => m,
        Value::Null => Map::new(),
        _ => return failed("The arguments must be a JSON object."),
    };
    match run(service, name, args).await {
        Ok(text) => done(text),
        Err(text) => failed(&text),
    }
}

/// Copies the arguments that are present, renaming them to the core's names.
fn pick(args: &Map<String, Value>, pairs: &[(&str, &str)]) -> Value {
    let mut out = Map::new();
    for (from, to) in pairs {
        if let Some(v) = args.get(*from) {
            if !v.is_null() {
                out.insert((*to).to_string(), v.clone());
            }
        }
    }
    Value::Object(out)
}

fn normalize(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// The configured project whose local clone contains `cwd`, if any.
fn project_for_folder(cfg: &Config, cwd: &Path) -> Option<String> {
    let here = normalize(cwd);
    cfg.projects
        .iter()
        .filter_map(|p| {
            let root = normalize(Path::new(p.local_path.as_ref()?));
            (here == root || here.starts_with(&format!("{root}/"))).then(|| (root.len(), p.id.clone()))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, id)| id)
}

async fn run(service: &Service, name: &str, mut args: Map<String, Value>) -> Result<String, String> {
    let has_project = args.get("project").and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty());
    if !has_project {
        args.remove("project");
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(id) = project_for_folder(&Config::load(&config::config_path()), &cwd) {
                args.insert("project".into(), json!(id));
            }
        }
    }
    let api = |method: &'static str, params: Value| async move { service.call(method, params).await.map_err(|e| e.to_string()) };
    let project = pick(&args, &[("project", "project")]);

    match name {
        "list_projects" => Ok(format::projects(&api("projects_list", Value::Null).await?)),
        "list_tasks" => {
            let params = pick(&args, &[("project", "project"), ("status", "status"), ("assignee", "assignee"), ("search", "search"), ("include_done", "includeDone")]);
            Ok(format::task_list(&api("tasks_list", params).await?))
        }
        "get_task" => {
            let params = pick(&args, &[("project", "project"), ("number", "number")]);
            Ok(format::task_detail(&api("task_get", params).await?))
        }
        "create_task" => {
            let params = pick(&args, &[("project", "project"), ("title", "title"), ("description", "body"), ("status", "status"), ("priority", "priority"), ("due", "due"), ("assignees", "assignees")]);
            let task = api("task_create", params).await?;
            Ok(format!("Created task #{}.\n\n{}", task["number"], format::task_detail(&task)))
        }
        "update_task" => {
            let params = pick(&args, &[("project", "project"), ("number", "number"), ("title", "title"), ("description", "body"), ("status", "status"), ("priority", "priority"), ("due", "due"), ("assignees", "assignees")]);
            if params.as_object().is_some_and(|m| m.keys().all(|k| k == "project" || k == "number")) {
                return Err("Nothing to change: pass at least one of status, priority, due, assignees, title or description.".into());
            }
            let task = api("task_update", params).await?;
            Ok(format!("Updated task #{}.\n\n{}", task["number"], format::task_detail(&task)))
        }
        "add_comment" => {
            let params = pick(&args, &[("project", "project"), ("number", "number"), ("body", "body")]);
            let comment = api("comment_add", params).await?;
            Ok(format!("Comment added to task #{}: {}", args.get("number").cloned().unwrap_or(Value::Null), comment["url"].as_str().unwrap_or("")))
        }
        "list_members" => Ok(format::members(&api("snapshot", project).await?)),
        "list_pull_requests" => {
            let state = args.get("state").and_then(Value::as_str).unwrap_or("open");
            if !["open", "merged", "closed", "all"].contains(&state) {
                return Err(format!("Unknown state \"{state}\". Use open, merged, closed or all."));
            }
            Ok(format::pulls(&api("snapshot", project).await?, state))
        }
        "link_pull_request" => {
            let params = pick(&args, &[("project", "project"), ("pull", "pull"), ("task", "task")]);
            let r = api("pull_link", params).await?;
            let mut text = if r["changed"].as_bool().unwrap_or(false) {
                format!("Linked: pull request #{} now closes task #{}.", args["pull"], args["task"])
            } else {
                format!("Pull request #{} was already linked to task #{}.", args["pull"], args["task"])
            };
            if let Some(note) = r["note"].as_str() {
                text.push_str("\nNote: ");
                text.push_str(note);
            }
            Ok(text)
        }
        "start_branch" => {
            let params = pick(&args, &[("project", "project"), ("number", "number"), ("name", "name")]);
            let r = api("task_start_branch", params).await?;
            let branch = r["branch"].as_str().unwrap_or("");
            Ok(format!(
                "Created branch \"{branch}\" from {} on GitHub, linked to task #{}.\nTo work on it locally: git fetch origin && git switch {branch}",
                r["base"].as_str().unwrap_or("the default branch"),
                args["number"]
            ))
        }
        "project_status" => Ok(format::status(&api("snapshot", project).await?)),
        other => Err(format!("Unknown tool: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use headway_core::config::Project;

    #[test]
    fn every_tool_has_a_valid_schema() {
        let defs = definitions();
        assert_eq!(defs.len(), 11);
        for d in &defs {
            let schema = &d["inputSchema"];
            assert_eq!(schema["type"], "object");
            let props = schema["properties"].as_object().unwrap();
            for r in schema["required"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
                assert!(props.contains_key(r.as_str().unwrap()), "{} requires a property it does not define", d["name"]);
            }
            assert!(exists(d["name"].as_str().unwrap()));
        }
        assert!(!exists("drop_database"));
    }

    #[test]
    fn picks_and_renames_present_arguments_only() {
        let args: Map<String, Value> = serde_json::from_value(json!({ "description": "x", "include_done": true, "due": null })).unwrap();
        let got = pick(&args, &[("description", "body"), ("include_done", "includeDone"), ("due", "due"), ("title", "title")]);
        assert_eq!(got, json!({ "body": "x", "includeDone": true }));
    }

    #[test]
    fn finds_the_project_of_the_working_folder() {
        let project = |id: &str, path: Option<&str>| Project {
            id: id.into(),
            owner: "o".into(),
            repo: "r".into(),
            local_path: path.map(str::to_string),
            added_at: String::new(),
        };
        let cfg = Config {
            projects: vec![project("o/outer", Some("/work/outer")), project("o/inner", Some("/work/outer/vendor/inner")), project("o/none", None)],
            ..Default::default()
        };
        assert_eq!(project_for_folder(&cfg, Path::new("/work/outer/src")).as_deref(), Some("o/outer"));
        assert_eq!(project_for_folder(&cfg, Path::new("/work/outer/vendor/inner/src")).as_deref(), Some("o/inner"));
        assert_eq!(project_for_folder(&cfg, Path::new("/work/outer")).as_deref(), Some("o/outer"));
        assert_eq!(project_for_folder(&cfg, Path::new("/work/outer-two")), None);
        assert_eq!(project_for_folder(&cfg, Path::new("/elsewhere")), None);
    }
}
