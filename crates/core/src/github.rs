//! The GitHub API client and the handful of queries Headway makes.
//!
//! Reads go through GraphQL (one request brings tasks, their linked pull
//! requests and the labels). Writes to issues go through REST because it takes
//! label names and logins directly.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::{header, Method, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::model::*;
use crate::tasks::{self, LabelKind};
use crate::{Error, Result};

const API: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("headway-app/", env!("CARGO_PKG_VERSION"));

/// Days of commit history shown on the Project health screen.
pub const ACTIVITY_DAYS: i64 = 28;
/// Open issues beyond this are not loaded (the snapshot says so).
const OPEN_ISSUE_CAP: usize = 500;
/// How many closed issues and pull requests (most recently updated first) are kept.
pub const CLOSED_ISSUE_KEEP: usize = 100;
pub const PULL_KEEP: usize = 60;
/// Page sizes. A busy repository makes GitHub time out on large pages, so
/// pages are modest and are halved when GitHub answers "too slow".
const ISSUE_PAGE: usize = 50;
const PULL_PAGE: usize = 30;
const MIN_PAGE: usize = 10;
const RECENT_COMMIT_CAP: usize = 1000;

const F_ISSUE: &str = include_str!("gql/issue_fields.graphql");
const F_PULL: &str = include_str!("gql/pull_fields.graphql");
const F_COMMENT: &str = include_str!("gql/comment_fields.graphql");
const Q_WORK_META: &str = include_str!("gql/work_meta.graphql");
const Q_ISSUES: &str = include_str!("gql/issues.graphql");
const Q_PULLS: &str = include_str!("gql/pulls.graphql");
const Q_TASK: &str = include_str!("gql/task.graphql");
const Q_REPO: &str = include_str!("gql/repo.graphql");
const Q_MORE_COMMITS: &str = include_str!("gql/more_commits.graphql");
const Q_MEMBERS: &str = include_str!("gql/members.graphql");
const Q_REPOS: &str = include_str!("gql/repos.graphql");

const Q_VIEWER: &str = "query { viewer { login name avatarUrl url } }";
const Q_BRANCH_BASE: &str = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { id defaultBranchRef { name target { oid } } } }";
const M_ADD_COMMENT: &str = "mutation($subject: ID!, $body: String!) { addComment(input: {subjectId: $subject, body: $body}) { commentEdge { node { ...CommentFields } } } }";
const M_UPDATE_COMMENT: &str = "mutation($id: ID!, $body: String!) { updateIssueComment(input: {id: $id, body: $body}) { issueComment { ...CommentFields } } }";
const M_DELETE_COMMENT: &str = "mutation($id: ID!) { deleteIssueComment(input: {id: $id}) { clientMutationId } }";
const M_LINK_BRANCH: &str = "mutation($issue: ID!, $oid: GitObjectID!, $name: String!, $repo: ID!) { createLinkedBranch(input: {issueId: $issue, oid: $oid, name: $name, repositoryId: $repo}) { linkedBranch { ref { name } } } }";

#[derive(Clone)]
pub struct GitHub {
    http: reqwest::Client,
    token: Arc<str>,
    /// When the last write went out: GitHub asks integrations to leave at
    /// least one second between requests that change something.
    last_write: Arc<tokio::sync::Mutex<Option<Instant>>>,
}

pub enum Probe {
    Unchanged,
    Changed(Option<String>),
}

/// Tasks, pull requests and labels of one repository.
pub struct Work {
    pub viewer: User,
    pub tasks: Vec<Task>,
    pub pulls: Vec<Pull>,
    pub labels: Vec<Label>,
    pub truncated: bool,
    pub counts: Counts,
}

/// Totals that move with every task or pull request change.
#[derive(Clone, Copy, Default)]
pub struct Counts {
    pub open_issues: u32,
    pub closed_issues: u32,
    pub open_pulls: u32,
    pub merged_pulls: u32,
    pub closed_pulls: u32,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoChoice {
    pub name_with_owner: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub is_archived: bool,
    pub pushed_at: Option<String>,
    pub viewer_permission: Option<String>,
    pub owner_avatar: String,
}

/// An issue as REST returns it, reduced to what an update needs.
pub struct RawIssue {
    pub id: String,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub closed: bool,
}

impl GitHub {
    pub fn new(token: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(GitHub { http, token: Arc::from(token), last_write: Arc::new(tokio::sync::Mutex::new(None)) })
    }

    fn request(&self, method: Method, url: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, url)
            .bearer_auth(&*self.token)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    async fn pace_write(&self) {
        let mut last = self.last_write.lock().await;
        if let Some(t) = *last {
            let gap = Duration::from_millis(1000);
            let since = t.elapsed();
            if since < gap {
                tokio::time::sleep(gap - since).await;
            }
        }
        *last = Some(Instant::now());
    }

    async fn graphql(&self, query: &str, variables: Value, write: bool) -> Result<(Value, Vec<Value>)> {
        if write {
            self.pace_write().await;
        }
        let resp = self
            .request(Method::POST, &format!("{API}/graphql"))
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(http_error(status, &headers, &text));
        }
        let mut v: Value = serde_json::from_str(&text)?;
        let errors = match v.get_mut("errors").map(Value::take) {
            Some(Value::Array(a)) => a,
            _ => Vec::new(),
        };
        let data = v.get_mut("data").map(Value::take).unwrap_or(Value::Null);
        if data.is_null() {
            return Err(errors
                .first()
                .map(gql_error)
                .unwrap_or_else(|| Error::GitHub("GitHub sent an empty answer.".into())));
        }
        Ok((data, errors))
    }

    /// A read that fails if GitHub reports any error.
    async fn query(&self, query: &str, variables: Value) -> Result<Value> {
        let (data, errors) = self.graphql(query, variables, false).await?;
        match errors.first() {
            Some(e) => Err(gql_error(e)),
            None => Ok(data),
        }
    }

    /// A read where some fields may be refused (they come back as null).
    async fn query_lenient(&self, query: &str, variables: Value) -> Result<Value> {
        Ok(self.graphql(query, variables, false).await?.0)
    }

    async fn mutate(&self, query: &str, variables: Value) -> Result<Value> {
        let (data, errors) = self.graphql(query, variables, true).await?;
        match errors.first() {
            Some(e) => Err(gql_error(e)),
            None => Ok(data),
        }
    }

    async fn rest(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        if method != Method::GET {
            self.pace_write().await;
        }
        let mut req = self.request(method, &format!("{API}{path}"));
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(http_error(status, &headers, &text));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    // ---- reads -----------------------------------------------------------

    pub async fn viewer(&self) -> Result<User> {
        let mut data = self.query(Q_VIEWER, json!({})).await?;
        let actor: GActor = serde_json::from_value(data["viewer"].take())?;
        Ok(actor.into())
    }

    /// Path whose answer changes whenever a task or pull request changes: the
    /// most recently updated issue (GitHub lists pull requests there too).
    pub fn work_probe_path(owner: &str, name: &str) -> String {
        format!("/repos/{owner}/{name}/issues?state=all&sort=updated&direction=desc&per_page=1")
    }

    /// Path whose answer changes when someone pushes (its `pushed_at` moves).
    pub fn repo_probe_path(owner: &str, name: &str) -> String {
        format!("/repos/{owner}/{name}")
    }

    /// A cheap "did anything change?" check: the request carries the previous
    /// ETag, and GitHub answers 304 (which does not count against the rate
    /// limit) when nothing moved.
    pub async fn probe(&self, path: &str, etag: Option<&str>) -> Result<Probe> {
        let url = format!("{API}{path}");
        let mut req = self.request(Method::GET, &url);
        if let Some(e) = etag {
            req = req.header(header::IF_NONE_MATCH, e);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if status == StatusCode::NOT_MODIFIED {
            return Ok(Probe::Unchanged);
        }
        let headers = resp.headers().clone();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(http_error(status, &headers, &text));
        }
        let etag = headers.get(header::ETAG).and_then(|v| v.to_str().ok()).map(str::to_string);
        Ok(Probe::Changed(etag))
    }

    /// Reads one connection page by page until `want` items are in hand.
    /// Returns the items and whether GitHub has more beyond them.
    async fn paged<T: serde::de::DeserializeOwned>(
        &self,
        query: &str,
        mut vars: Value,
        field: &str,
        page_size: usize,
        want: usize,
    ) -> Result<(Vec<T>, bool)> {
        let mut out: Vec<T> = Vec::new();
        let mut page = page_size;
        let mut after: Option<String> = None;
        let mut retries = 0;
        loop {
            vars["first"] = json!(page.min(want - out.len()).max(1));
            vars["after"] = json!(after);
            let mut data = match self.query(query, vars.clone()).await {
                Ok(d) => d,
                // GitHub gave up on the page: ask for a smaller one.
                Err(Error::Overloaded) if page > MIN_PAGE && retries < 4 => {
                    page = (page / 2).max(MIN_PAGE);
                    retries += 1;
                    continue;
                }
                Err(e) => return Err(e),
            };
            let repo = &mut data["repository"];
            if repo.is_null() {
                return Err(Error::NotFound("GitHub could not find that repository, or this account cannot see it.".into()));
            }
            let conn: Conn<T> = serde_json::from_value(repo[field].take())?;
            let info = conn.page_info.clone();
            out.extend(conn.items());
            match info {
                Some(PageInfo { has_next_page: true, end_cursor: Some(cursor) }) if out.len() < want => after = Some(cursor),
                Some(PageInfo { has_next_page, .. }) => return Ok((out, has_next_page)),
                None => return Ok((out, false)),
            }
        }
    }

    async fn issues(&self, owner: &str, name: &str, states: Option<&str>, since: Option<&str>, want: usize) -> Result<(Vec<Task>, bool)> {
        let q = [Q_ISSUES, F_ISSUE].join("\n");
        let vars = json!({ "owner": owner, "name": name, "states": states.map(|s| vec![s]), "since": since });
        let (items, more) = self.paged::<GIssue>(&q, vars, "issues", ISSUE_PAGE, want).await?;
        Ok((items.into_iter().map(to_task).collect(), more))
    }

    async fn pulls(&self, owner: &str, name: &str, want: usize) -> Result<Vec<Pull>> {
        let q = [Q_PULLS, F_PULL].join("\n");
        let vars = json!({ "owner": owner, "name": name });
        let (items, _) = self.paged::<GPull>(&q, vars, "pullRequests", PULL_PAGE, want).await?;
        Ok(items.into_iter().map(to_pull).collect())
    }

    async fn work_meta(&self, owner: &str, name: &str) -> Result<(User, Counts, Vec<Label>)> {
        let data = self.query(Q_WORK_META, json!({ "owner": owner, "name": name })).await?;
        let d: GWorkMeta = serde_json::from_value(data)?;
        let repo = d.repository.ok_or_else(|| repo_not_found(owner, name))?;
        let counts = Counts {
            open_issues: repo.open_issues.total_count,
            closed_issues: repo.closed_issues.total_count,
            open_pulls: repo.open_pulls.total_count,
            merged_pulls: repo.merged_pulls.total_count,
            closed_pulls: repo.closed_pulls.total_count,
        };
        let labels = repo.labels.map(Conn::items).unwrap_or_default().into_iter().map(|l| Label { name: l.name, color: l.color }).collect();
        Ok((d.viewer.into(), counts, labels))
    }

    /// Everything: all open issues (up to the cap), the most recently updated
    /// closed ones, and the most recently updated pull requests.
    pub async fn work(&self, owner: &str, name: &str) -> Result<Work> {
        let (viewer, counts, labels) = self.work_meta(owner, name).await?;
        let (mut tasks, truncated) = self.issues(owner, name, Some("OPEN"), None, OPEN_ISSUE_CAP).await?;
        let (closed, _) = self.issues(owner, name, Some("CLOSED"), None, CLOSED_ISSUE_KEEP).await?;
        tasks.extend(closed);
        let pulls = self.pulls(owner, name, PULL_KEEP).await?;
        Ok(Work { viewer, tasks, pulls, labels, truncated, counts })
    }

    /// Only what changed since `since` (an ISO time): the issues updated after
    /// it, in any state, and the first page of recently updated pull requests.
    /// The caller merges these into what it already has.
    pub async fn work_since(&self, owner: &str, name: &str, since: &str) -> Result<Work> {
        let (viewer, counts, labels) = self.work_meta(owner, name).await?;
        let (tasks, truncated) = self.issues(owner, name, None, Some(since), 300).await?;
        let pulls = self.pulls(owner, name, PULL_PAGE).await?;
        Ok(Work { viewer, tasks, pulls, labels, truncated, counts })
    }

    pub async fn task(&self, owner: &str, name: &str, number: u64) -> Result<TaskDetail> {
        let q = [Q_TASK, F_ISSUE, F_COMMENT].join("\n");
        let mut data = self.query(&q, json!({ "owner": owner, "name": name, "number": number })).await?;
        let mut issue: GIssue = match serde_json::from_value::<Option<GIssue>>(data["repository"]["issue"].take())? {
            Some(i) => i,
            None => return Err(Error::NotFound(format!("There is no task #{number} in {owner}/{name}."))),
        };
        let comments = issue.thread.take().map(Conn::items).unwrap_or_default().into_iter().map(to_comment).collect();
        Ok(TaskDetail { task: to_task(issue), comments })
    }

    pub async fn repo(&self, owner: &str, name: &str) -> Result<RepoStatus> {
        let since = (chrono::Utc::now() - chrono::Duration::days(ACTIVITY_DAYS))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let data = self.query(Q_REPO, json!({ "owner": owner, "name": name, "since": since })).await?;
        let d: GRepoData = serde_json::from_value(data)?;
        let r = d.repository.ok_or_else(|| repo_not_found(owner, name))?;

        let mut last_commit = None;
        let mut total_commits = 0;
        let mut recent: Vec<CommitStamp> = Vec::new();
        let mut recent_truncated = false;
        let default_branch = r.default_branch_ref.as_ref().map(|b| b.name.clone());
        if let Some(target) = r.default_branch_ref.and_then(|b| b.target) {
            if let Some(h) = target.history {
                total_commits = h.total_count;
                last_commit = h.nodes.unwrap_or_default().into_iter().flatten().next().map(to_commit_info);
            }
            if let Some(h) = target.recent {
                let mut page = h.page_info.clone();
                recent.extend(h.nodes.unwrap_or_default().into_iter().flatten().map(to_stamp));
                while let Some(PageInfo { has_next_page: true, end_cursor: Some(cursor) }) = page {
                    if recent.len() >= RECENT_COMMIT_CAP {
                        recent_truncated = true;
                        break;
                    }
                    let vars = json!({ "owner": owner, "name": name, "since": since, "after": cursor });
                    let mut more = self.query(Q_MORE_COMMITS, vars).await?;
                    let h: GHistory =
                        serde_json::from_value(more["repository"]["defaultBranchRef"]["target"]["recent"].take())?;
                    page = h.page_info.clone();
                    recent.extend(h.nodes.unwrap_or_default().into_iter().flatten().map(to_stamp));
                }
            }
        }

        Ok(RepoStatus {
            id: r.id,
            name_with_owner: r.name_with_owner,
            name: r.name,
            owner_login: r.owner.login,
            owner_avatar: r.owner.avatar_url.unwrap_or_default(),
            description: r.description.filter(|s| !s.trim().is_empty()),
            url: r.url,
            homepage_url: r.homepage_url.filter(|s| !s.trim().is_empty()),
            is_private: r.is_private,
            is_archived: r.is_archived,
            has_issues: r.has_issues_enabled,
            created_at: r.created_at,
            pushed_at: r.pushed_at,
            default_branch,
            last_commit,
            total_commits,
            branch_count: r.branches.map(|c| c.total_count).unwrap_or(0),
            open_issues: r.open_issues.total_count,
            closed_issues: r.closed_issues.total_count,
            open_pulls: r.open_pulls.total_count,
            merged_pulls: r.merged_pulls.total_count,
            closed_pulls: r.closed_pulls.total_count,
            stars: r.stargazer_count,
            forks: r.fork_count,
            primary_language: r.primary_language.map(|l| Language { name: l.name, color: l.color }),
            viewer_permission: r.viewer_permission,
            recent_commits: recent,
            recent_commits_truncated: recent_truncated,
        })
    }

    /// The people who can be given tasks. Roles need push access to read; without
    /// it we still get the names, with the role left as "member".
    pub async fn members(&self, owner: &str, name: &str) -> Result<Vec<Member>> {
        let data = self.query_lenient(Q_MEMBERS, json!({ "owner": owner, "name": name })).await?;
        let repo = &data["repository"];
        if repo.is_null() {
            return Err(repo_not_found(owner, name));
        }
        let mut members: Vec<Member> = Vec::new();
        if let Some(edges) = repo.pointer("/collaborators/edges").and_then(Value::as_array) {
            for e in edges {
                let Ok(actor) = serde_json::from_value::<GActor>(e["node"].clone()) else { continue };
                let role = e["permission"].as_str().unwrap_or("READ").to_ascii_lowercase();
                members.push(member(actor, role));
            }
        } else if let Some(nodes) = repo.pointer("/assignableUsers/nodes").and_then(Value::as_array) {
            for n in nodes {
                let Ok(actor) = serde_json::from_value::<GActor>(n.clone()) else { continue };
                let role = if actor.login.eq_ignore_ascii_case(owner) { "admin" } else { "member" };
                members.push(member(actor, role.to_string()));
            }
        }
        let rank = |role: &str| match role {
            "admin" => 0,
            "maintain" => 1,
            "write" => 2,
            "member" => 3,
            "triage" => 4,
            _ => 5,
        };
        members.sort_by(|a, b| {
            rank(&a.role).cmp(&rank(&b.role)).then_with(|| a.login.to_lowercase().cmp(&b.login.to_lowercase()))
        });
        Ok(members)
    }

    /// The signed-in user's repositories, most recently pushed first.
    pub async fn repos(&self) -> Result<Vec<RepoChoice>> {
        let data = self.query_lenient(Q_REPOS, json!({})).await?;
        let nodes = data.pointer("/viewer/repositories/nodes").and_then(Value::as_array).cloned().unwrap_or_default();
        Ok(nodes
            .into_iter()
            .filter_map(|n| {
                Some(RepoChoice {
                    name_with_owner: n.get("nameWithOwner")?.as_str()?.to_string(),
                    description: n.get("description").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
                    is_private: n.get("isPrivate").and_then(Value::as_bool).unwrap_or(false),
                    is_archived: n.get("isArchived").and_then(Value::as_bool).unwrap_or(false),
                    pushed_at: n.get("pushedAt").and_then(Value::as_str).map(str::to_string),
                    viewer_permission: n.get("viewerPermission").and_then(Value::as_str).map(str::to_string),
                    owner_avatar: n.pointer("/owner/avatarUrl").and_then(Value::as_str).unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    // ---- writes ----------------------------------------------------------

    pub async fn create_issue(
        &self,
        owner: &str,
        name: &str,
        title: &str,
        body: &str,
        assignees: &[String],
        labels: &[String],
    ) -> Result<u64> {
        let payload = json!({ "title": title, "body": body, "assignees": assignees, "labels": labels });
        let v = self.rest(Method::POST, &format!("/repos/{owner}/{name}/issues"), Some(payload)).await?;
        v["number"].as_u64().ok_or_else(|| Error::GitHub("GitHub created the issue but did not return its number.".into()))
    }

    pub async fn issue_raw(&self, owner: &str, name: &str, number: u64) -> Result<RawIssue> {
        let v = self.rest(Method::GET, &format!("/repos/{owner}/{name}/issues/{number}"), None).await?;
        if v.get("pull_request").is_some() {
            return Err(Error::invalid(format!("#{number} is a pull request, not a task.")));
        }
        Ok(RawIssue {
            id: v["node_id"].as_str().unwrap_or_default().to_string(),
            title: v["title"].as_str().unwrap_or_default().to_string(),
            body: v["body"].as_str().unwrap_or_default().to_string(),
            labels: v["labels"]
                .as_array()
                .map(|a| a.iter().filter_map(|l| l["name"].as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            closed: v["state"].as_str() == Some("closed"),
        })
    }

    pub async fn patch_issue(&self, owner: &str, name: &str, number: u64, patch: Value) -> Result<()> {
        self.rest(Method::PATCH, &format!("/repos/{owner}/{name}/issues/{number}"), Some(patch)).await?;
        Ok(())
    }

    /// Creates Headway's labels that the repo does not have yet (with their
    /// colours). Without this GitHub would create them grey on first use.
    pub async fn ensure_labels(&self, owner: &str, name: &str, have: &[String], want: &[String]) -> Result<()> {
        for w in want {
            if have.iter().any(|h| h.eq_ignore_ascii_case(w)) {
                continue;
            }
            let Some(spec) = tasks::MANAGED_LABELS.iter().find(|s| s.name == w) else { continue };
            let payload = json!({ "name": spec.name, "color": spec.color, "description": spec.description });
            match self.rest(Method::POST, &format!("/repos/{owner}/{name}/labels"), Some(payload)).await {
                // 422 = it already exists (someone else created it meanwhile).
                Ok(_) | Err(Error::Invalid(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    pub async fn add_comment(&self, subject_id: &str, body: &str) -> Result<Comment> {
        let q = [M_ADD_COMMENT, F_COMMENT].join("\n");
        let mut data = self.mutate(&q, json!({ "subject": subject_id, "body": body })).await?;
        let c: GComment = serde_json::from_value(data["addComment"]["commentEdge"]["node"].take())?;
        Ok(to_comment(c))
    }

    pub async fn update_comment(&self, id: &str, body: &str) -> Result<Comment> {
        let q = [M_UPDATE_COMMENT, F_COMMENT].join("\n");
        let mut data = self.mutate(&q, json!({ "id": id, "body": body })).await?;
        let c: GComment = serde_json::from_value(data["updateIssueComment"]["issueComment"].take())?;
        Ok(to_comment(c))
    }

    pub async fn delete_comment(&self, id: &str) -> Result<()> {
        self.mutate(M_DELETE_COMMENT, json!({ "id": id })).await?;
        Ok(())
    }

    /// (repository id, default branch name, its head commit)
    pub async fn branch_base(&self, owner: &str, name: &str) -> Result<(String, String, String)> {
        let data = self.query(Q_BRANCH_BASE, json!({ "owner": owner, "name": name })).await?;
        let repo = &data["repository"];
        let id = repo["id"].as_str().ok_or_else(|| repo_not_found(owner, name))?;
        let branch = repo.pointer("/defaultBranchRef/name").and_then(Value::as_str);
        let oid = repo.pointer("/defaultBranchRef/target/oid").and_then(Value::as_str);
        match (branch, oid) {
            (Some(b), Some(o)) => Ok((id.to_string(), b.to_string(), o.to_string())),
            _ => Err(Error::invalid("This repository has no commits yet, so there is nothing to branch from.")),
        }
    }

    /// Creates a branch on GitHub that is linked to the issue: a pull request
    /// opened from it is attached to the task automatically.
    pub async fn create_linked_branch(&self, issue_id: &str, repo_id: &str, oid: &str, branch: &str) -> Result<String> {
        let vars = json!({ "issue": issue_id, "oid": oid, "name": branch, "repo": repo_id });
        let data = self.mutate(M_LINK_BRANCH, vars).await?;
        match data.pointer("/createLinkedBranch/linkedBranch/ref/name").and_then(Value::as_str) {
            Some(n) => Ok(n.to_string()),
            // GitHub answers null here when the branch name is already taken.
            None => Err(Error::invalid(format!("GitHub did not create the branch \"{branch}\" (the name may already exist)."))),
        }
    }

    /// (description, base branch) of a pull request.
    pub async fn pull_body(&self, owner: &str, name: &str, number: u64) -> Result<(String, String)> {
        let v = self.rest(Method::GET, &format!("/repos/{owner}/{name}/pulls/{number}"), None).await?;
        Ok((
            v["body"].as_str().unwrap_or_default().to_string(),
            v.pointer("/base/ref").and_then(Value::as_str).unwrap_or_default().to_string(),
        ))
    }

    pub async fn set_pull_body(&self, owner: &str, name: &str, number: u64, body: &str) -> Result<()> {
        self.rest(Method::PATCH, &format!("/repos/{owner}/{name}/pulls/{number}"), Some(json!({ "body": body }))).await?;
        Ok(())
    }

    pub async fn create_pull(
        &self,
        owner: &str,
        name: &str,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
        draft: bool,
    ) -> Result<(u64, String)> {
        let payload = json!({ "title": title, "head": head, "base": base, "body": body, "draft": draft });
        let v = self.rest(Method::POST, &format!("/repos/{owner}/{name}/pulls"), Some(payload)).await?;
        let number = v["number"].as_u64().ok_or_else(|| Error::GitHub("GitHub did not return the pull request number.".into()))?;
        Ok((number, v["html_url"].as_str().unwrap_or_default().to_string()))
    }
}

fn repo_not_found(owner: &str, name: &str) -> Error {
    Error::NotFound(format!(
        "GitHub has no repository {owner}/{name}, or the signed-in account cannot see it."
    ))
}

fn http_error(status: StatusCode, headers: &header::HeaderMap, body: &str) -> Error {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let mut msg = v.get("message").and_then(Value::as_str).unwrap_or("").to_string();
    if let Some(errs) = v.get("errors").and_then(Value::as_array) {
        let details: Vec<String> = errs
            .iter()
            .filter_map(|e| {
                if let Some(m) = e.get("message").and_then(Value::as_str) {
                    return Some(m.to_string());
                }
                let field = e.get("field").and_then(Value::as_str)?;
                let code = e.get("code").and_then(Value::as_str)?;
                Some(format!("{field}: {code}"))
            })
            .collect();
        if !details.is_empty() {
            msg = format!("{msg} ({})", details.join("; "));
        }
    }
    if msg.is_empty() {
        msg = format!("GitHub answered {status}");
    }
    let h = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let limited = h("x-ratelimit-remaining") == Some("0")
        || h("retry-after").is_some()
        || msg.to_lowercase().contains("rate limit");
    match status.as_u16() {
        401 => Error::Unauthorized,
        403 | 429 if limited => Error::RateLimited(rate_hint(h("x-ratelimit-reset"), h("retry-after"))),
        403 => Error::Forbidden(msg),
        404 => Error::NotFound("GitHub could not find that, or this account has no access to it.".into()),
        422 => Error::Invalid(msg),
        // GitHub answers 502/504 when a query took too long on its side.
        502..=504 => Error::Overloaded,
        _ => Error::GitHub(msg),
    }
}

fn rate_hint(reset: Option<&str>, retry_after: Option<&str>) -> String {
    if let Some(s) = retry_after.and_then(|s| s.parse::<u64>().ok()) {
        // i18n
        return format!("Try again in {s} seconds.");
    }
    if let Some(ts) = reset.and_then(|s| s.parse::<i64>().ok()) {
        let secs = (ts - chrono::Utc::now().timestamp()).max(0);
        // i18n
        return format!("Try again in about {} minutes.", secs / 60 + 1);
    }
    // i18n
    "Try again in a few minutes.".into()
}

fn gql_error(e: &Value) -> Error {
    let msg = e.get("message").and_then(Value::as_str).unwrap_or("unknown error").to_string();
    let lower = msg.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("something went wrong while executing your query") {
        return Error::Overloaded;
    }
    match e.get("type").and_then(Value::as_str) {
        Some("NOT_FOUND") => Error::NotFound(msg),
        Some("FORBIDDEN") | Some("INSUFFICIENT_SCOPES") => Error::Forbidden(msg),
        Some("RATE_LIMITED") => Error::RateLimited("Try again in a few minutes.".into()),
        Some("UNPROCESSABLE") => Error::Invalid(msg),
        _ => Error::GitHub(msg),
    }
}

// ---- GraphQL response shapes ---------------------------------------------

#[derive(Deserialize)]
struct Conn<T> {
    nodes: Option<Vec<Option<T>>>,
    #[serde(rename = "totalCount")]
    #[allow(dead_code)]
    total_count: Option<u32>,
    #[serde(rename = "pageInfo")]
    page_info: Option<PageInfo>,
}

impl<T> Conn<T> {
    fn items(self) -> Vec<T> {
        self.nodes.unwrap_or_default().into_iter().flatten().collect()
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GCount {
    total_count: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GActor {
    login: String,
    name: Option<String>,
    #[serde(default)]
    avatar_url: String,
    #[serde(default)]
    url: String,
}

impl From<GActor> for User {
    fn from(a: GActor) -> User {
        User { login: a.login, name: a.name.filter(|n| !n.trim().is_empty()), avatar_url: a.avatar_url, url: a.url }
    }
}

fn member(a: GActor, role: String) -> Member {
    Member { login: a.login, name: a.name.filter(|n| !n.trim().is_empty()), avatar_url: a.avatar_url, url: a.url, role }
}

#[derive(Deserialize)]
struct GLabel {
    name: String,
    #[serde(default)]
    color: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GPullRef {
    number: u64,
    title: String,
    url: String,
    state: String,
    is_draft: bool,
}

#[derive(Deserialize)]
struct GName {
    name: String,
}

#[derive(Deserialize)]
struct GLinkedBranch {
    #[serde(rename = "ref")]
    reference: Option<GName>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GComment {
    id: String,
    url: String,
    #[serde(default)]
    body: String,
    #[serde(default, rename = "bodyHTML")]
    body_html: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    viewer_can_update: bool,
    #[serde(default)]
    viewer_can_delete: bool,
    #[serde(default)]
    viewer_did_author: bool,
    author: Option<GActor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GIssue {
    id: String,
    number: u64,
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default, rename = "bodyHTML")]
    body_html: Option<String>,
    url: String,
    state: String,
    state_reason: Option<String>,
    created_at: String,
    updated_at: String,
    closed_at: Option<String>,
    author: Option<GActor>,
    assignees: Option<Conn<GActor>>,
    labels: Option<Conn<GLabel>>,
    comments: Option<GCount>,
    closed_by_pull_requests_references: Option<Conn<GPullRef>>,
    linked_branches: Option<Conn<GLinkedBranch>>,
    thread: Option<Conn<GComment>>,
}

#[derive(Deserialize)]
struct GOid {
    oid: String,
}

#[derive(Deserialize)]
struct GNumber {
    number: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GMaybeUser {
    login: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
    url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GReviewRequest {
    requested_reviewer: Option<GMaybeUser>,
}

#[derive(Deserialize)]
struct GStateCount {
    state: String,
    count: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GContexts {
    #[serde(default)]
    total_count: u32,
    check_run_counts_by_state: Option<Vec<GStateCount>>,
    status_context_counts_by_state: Option<Vec<GStateCount>>,
}

#[derive(Deserialize)]
struct GRollup {
    state: String,
    contexts: Option<GContexts>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GCommitChecks {
    status_check_rollup: Option<GRollup>,
}

#[derive(Deserialize)]
struct GPullCommit {
    commit: GCommitChecks,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GPull {
    id: String,
    number: u64,
    title: String,
    url: String,
    state: String,
    is_draft: bool,
    created_at: String,
    updated_at: String,
    merged_at: Option<String>,
    closed_at: Option<String>,
    author: Option<GActor>,
    head_ref_name: String,
    base_ref_name: String,
    #[serde(default)]
    head_ref_oid: String,
    #[serde(default)]
    additions: u32,
    #[serde(default)]
    deletions: u32,
    #[serde(default)]
    changed_files: u32,
    review_decision: Option<String>,
    total_comments_count: Option<u32>,
    merge_commit: Option<GOid>,
    closing_issues_references: Option<Conn<GNumber>>,
    review_requests: Option<Conn<GReviewRequest>>,
    commits: Option<Conn<GPullCommit>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GWorkMetaRepo {
    open_issues: GCount,
    closed_issues: GCount,
    open_pulls: GCount,
    merged_pulls: GCount,
    closed_pulls: GCount,
    labels: Option<Conn<GLabel>>,
}

#[derive(Deserialize)]
struct GWorkMeta {
    viewer: GActor,
    repository: Option<GWorkMetaRepo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GOwner {
    login: String,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GLang {
    name: String,
    color: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GGitUser {
    login: String,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GGitActor {
    name: Option<String>,
    user: Option<GGitUser>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GCommit {
    oid: Option<String>,
    message_headline: Option<String>,
    committed_date: String,
    url: Option<String>,
    author: Option<GGitActor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GHistory {
    #[serde(default)]
    total_count: u64,
    page_info: Option<PageInfo>,
    nodes: Option<Vec<Option<GCommit>>>,
}

#[derive(Deserialize)]
struct GTarget {
    history: Option<GHistory>,
    recent: Option<GHistory>,
}

#[derive(Deserialize)]
struct GDefaultBranch {
    name: String,
    target: Option<GTarget>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GRepo {
    id: String,
    name_with_owner: String,
    name: String,
    description: Option<String>,
    url: String,
    homepage_url: Option<String>,
    is_private: bool,
    is_archived: bool,
    has_issues_enabled: bool,
    created_at: String,
    pushed_at: Option<String>,
    owner: GOwner,
    stargazer_count: u32,
    fork_count: u32,
    viewer_permission: Option<String>,
    primary_language: Option<GLang>,
    default_branch_ref: Option<GDefaultBranch>,
    open_issues: GCount,
    closed_issues: GCount,
    open_pulls: GCount,
    merged_pulls: GCount,
    closed_pulls: GCount,
    branches: Option<GCount>,
}

#[derive(Deserialize)]
struct GRepoData {
    repository: Option<GRepo>,
}

// ---- mapping to the model ------------------------------------------------

fn pull_state(s: &str) -> PullState {
    match s.to_ascii_uppercase().as_str() {
        "MERGED" => PullState::Merged,
        "CLOSED" => PullState::Closed,
        _ => PullState::Open,
    }
}

fn to_task(i: GIssue) -> Task {
    let label_nodes = i.labels.map(Conn::items).unwrap_or_default();
    let names: Vec<&str> = label_nodes.iter().map(|l| l.name.as_str()).collect();
    let closed = i.state.eq_ignore_ascii_case("CLOSED");
    let status = tasks::derive_status(closed, i.state_reason.as_deref(), &names);
    let priority = tasks::derive_priority(&names);
    let (body, meta) = tasks::split_body(&i.body);
    let labels = label_nodes
        .into_iter()
        .filter(|l| tasks::classify(&l.name) == LabelKind::Other)
        .map(|l| Label { name: l.name, color: l.color })
        .collect();
    Task {
        id: i.id,
        number: i.number,
        title: i.title,
        body,
        body_html: i.body_html,
        url: i.url,
        status,
        priority,
        due: meta.due(),
        assignees: i.assignees.map(Conn::items).unwrap_or_default().into_iter().map(User::from).collect(),
        author: i.author.map(User::from),
        labels,
        comment_count: i.comments.map(|c| c.total_count).unwrap_or(0),
        created_at: i.created_at,
        updated_at: i.updated_at,
        closed_at: i.closed_at,
        pulls: i
            .closed_by_pull_requests_references
            .map(Conn::items)
            .unwrap_or_default()
            .into_iter()
            .map(|p| PullRef { number: p.number, title: p.title, url: p.url, state: pull_state(&p.state), is_draft: p.is_draft })
            .collect(),
        branches: i
            .linked_branches
            .map(Conn::items)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|b| b.reference.map(|r| r.name))
            .collect(),
    }
}

fn to_comment(c: GComment) -> Comment {
    Comment {
        id: c.id,
        author: c.author.map(User::from),
        body: c.body,
        body_html: c.body_html,
        created_at: c.created_at,
        updated_at: c.updated_at,
        url: c.url,
        can_edit: c.viewer_can_update,
        can_delete: c.viewer_can_delete,
        is_mine: c.viewer_did_author,
    }
}

fn to_checks(r: GRollup) -> Checks {
    let (mut passed, mut failed, mut pending, mut counted) = (0u32, 0u32, 0u32, 0u32);
    let mut total = 0;
    if let Some(ctx) = r.contexts {
        total = ctx.total_count;
        for c in ctx.check_run_counts_by_state.unwrap_or_default() {
            counted += c.count;
            match c.state.as_str() {
                "SUCCESS" | "NEUTRAL" | "SKIPPED" => passed += c.count,
                "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" | "STALE" => failed += c.count,
                _ => pending += c.count,
            }
        }
        for c in ctx.status_context_counts_by_state.unwrap_or_default() {
            counted += c.count;
            match c.state.as_str() {
                "SUCCESS" => passed += c.count,
                "FAILURE" | "ERROR" => failed += c.count,
                _ => pending += c.count,
            }
        }
    }
    Checks { state: r.state.to_ascii_lowercase(), total: total.max(counted), passed, failed, pending }
}

fn to_pull(p: GPull) -> Pull {
    let checks = p
        .commits
        .map(Conn::items)
        .unwrap_or_default()
        .into_iter()
        .next()
        .and_then(|c| c.commit.status_check_rollup)
        .map(to_checks);
    let reviewers = p
        .review_requests
        .map(Conn::items)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| r.requested_reviewer)
        .filter_map(|u| {
            Some(User { login: u.login?, name: u.name, avatar_url: u.avatar_url.unwrap_or_default(), url: u.url.unwrap_or_default() })
        })
        .collect();
    Pull {
        id: p.id,
        number: p.number,
        title: p.title,
        url: p.url,
        state: pull_state(&p.state),
        is_draft: p.is_draft,
        author: p.author.map(User::from),
        head_ref: p.head_ref_name,
        base_ref: p.base_ref_name,
        head_oid: p.head_ref_oid,
        created_at: p.created_at,
        updated_at: p.updated_at,
        merged_at: p.merged_at,
        closed_at: p.closed_at,
        additions: p.additions,
        deletions: p.deletions,
        changed_files: p.changed_files,
        review: p.review_decision.map(|r| r.to_ascii_lowercase()),
        checks,
        tasks: p.closing_issues_references.map(Conn::items).unwrap_or_default().into_iter().map(|n| n.number).collect(),
        comments: p.total_comments_count.unwrap_or(0),
        reviewers,
        merge_commit: p.merge_commit.map(|m| m.oid),
    }
}

fn to_commit_info(c: GCommit) -> CommitInfo {
    let (author_name, author_login, author_avatar) = match c.author {
        Some(a) => {
            let (login, avatar) = match a.user {
                Some(u) => (Some(u.login), u.avatar_url),
                None => (None, None),
            };
            (a.name.unwrap_or_default(), login, avatar)
        }
        None => (String::new(), None, None),
    };
    CommitInfo {
        oid: c.oid.unwrap_or_default(),
        headline: c.message_headline.unwrap_or_default(),
        committed_at: c.committed_date,
        author_name,
        author_login,
        author_avatar,
        url: c.url.unwrap_or_default(),
    }
}

fn to_stamp(c: GCommit) -> CommitStamp {
    let (author_name, author_login) = match c.author {
        Some(a) => (a.name.unwrap_or_default(), a.user.map(|u| u.login)),
        None => (String::new(), None),
    };
    CommitStamp { committed_at: c.committed_date, author_login, author_name }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_an_issue_to_a_task() {
        let v = json!({
            "id": "I_1", "number": 7, "title": "Fix login", "url": "https://github.com/o/r/issues/7",
            "body": "Steps...\n\n<!-- headway: {\"due\":\"2026-12-03\"} -->",
            "state": "OPEN", "stateReason": null,
            "createdAt": "2026-11-20T10:00:00Z", "updatedAt": "2026-11-21T10:00:00Z", "closedAt": null,
            "author": {"login": "ana", "avatarUrl": "a.png", "url": "u", "name": "Ana"},
            "assignees": {"nodes": [{"login": "ben", "name": null, "avatarUrl": "b.png", "url": "u"}, null]},
            "labels": {"nodes": [{"name": "bug", "color": "d73a4a"}, {"name": "status: in review", "color": "8B5CF6"}, {"name": "priority: high", "color": "F97316"}]},
            "comments": {"totalCount": 3},
            "closedByPullRequestsReferences": {"nodes": [{"number": 9, "title": "Fix", "url": "p", "state": "OPEN", "isDraft": true}]},
            "linkedBranches": {"nodes": [{"ref": {"name": "7-fix-login"}}, {"ref": null}]}
        });
        let t = to_task(serde_json::from_value::<GIssue>(v).unwrap());
        assert_eq!(t.status, Status::InReview);
        assert_eq!(t.priority, Some(Priority::High));
        assert_eq!(t.due.as_deref(), Some("2026-12-03"));
        assert_eq!(t.body, "Steps...");
        assert_eq!(t.labels.len(), 1);
        assert_eq!(t.assignees.len(), 1);
        assert_eq!(t.comment_count, 3);
        assert_eq!(t.pulls[0].number, 9);
        assert!(t.pulls[0].is_draft);
        assert_eq!(t.branches, vec!["7-fix-login"]);
    }

    #[test]
    fn summarises_checks() {
        let r: GRollup = serde_json::from_value(json!({
            "state": "FAILURE",
            "contexts": {
                "totalCount": 6,
                "checkRunCountsByState": [{"state": "SUCCESS", "count": 3}, {"state": "FAILURE", "count": 1}, {"state": "IN_PROGRESS", "count": 1}],
                "statusContextCountsByState": [{"state": "SUCCESS", "count": 1}]
            }
        }))
        .unwrap();
        let c = to_checks(r);
        assert_eq!((c.total, c.passed, c.failed, c.pending), (6, 4, 1, 1));
        assert_eq!(c.state, "failure");
    }

    #[test]
    fn maps_http_errors() {
        let h = header::HeaderMap::new();
        assert_eq!(http_error(StatusCode::UNAUTHORIZED, &h, "{}").code(), "unauthorized");
        assert_eq!(http_error(StatusCode::NOT_FOUND, &h, "{}").code(), "not_found");
        let e = http_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            &h,
            r#"{"message":"Validation Failed","errors":[{"resource":"Label","code":"already_exists","field":"name"}]}"#,
        );
        assert_eq!(e.code(), "invalid");
        assert!(e.to_string().contains("name: already_exists"));
        let mut limited = header::HeaderMap::new();
        limited.insert("x-ratelimit-remaining", "0".parse().unwrap());
        assert_eq!(http_error(StatusCode::FORBIDDEN, &limited, "{}").code(), "rate_limited");
        assert_eq!(http_error(StatusCode::FORBIDDEN, &h, r#"{"message":"Must have admin rights"}"#).code(), "forbidden");
    }
}
