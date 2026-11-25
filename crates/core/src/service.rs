//! The one API that the desktop app, the MCP server and the dev server call:
//! `Service::call("method_name", json_params)`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Map, Value};
use tokio::sync::{Mutex as AsyncMutex, RwLock};

use std::sync::Arc;

use crate::auth::{self, DevicePoll, DeviceSession};
use crate::config::{self, AuthConfig, AuthSource, CalendarRef, Config, Project};
use crate::github::{GitHub, Probe};
use crate::model::*;
use crate::{calendar, git, proc, tasks, Error, Result};

/// A downloaded calendar is reused for this long before it is fetched again.
const CALENDAR_REFRESH: Duration = Duration::from_secs(300);
/// The Agenda never asks for more than this many days at once.
const CALENDAR_MAX_DAYS: i64 = 92;

/// Nothing tells us when the member list changes, so it is re-read this often.
const MEMBERS_REFRESH: Duration = Duration::from_secs(300);
/// Between full reads only changes are fetched; this bounds how long a
/// deleted or transferred issue could linger.
const FULL_RELOAD: Duration = Duration::from_secs(1800);

/// The moment to ask "what changed since?" for: a few minutes before our last
/// read, so clock differences and GitHub's own delay cannot hide a change.
fn since_with_margin(fetched_at: &str) -> Option<String> {
    let at = chrono::DateTime::parse_from_rfc3339(fetched_at).ok()?;
    let since = at.with_timezone(&chrono::Utc) - chrono::Duration::minutes(5);
    Some(since.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// Folds freshly changed tasks and pull requests into the previous snapshot.
fn merge_work(prev_tasks: &[Task], prev_pulls: &[Pull], truncated: bool, delta: crate::github::Work) -> crate::github::Work {
    use crate::github::{CLOSED_ISSUE_KEEP, PULL_KEEP};
    let mut tasks = prev_tasks.to_vec();
    for t in delta.tasks {
        match tasks.iter().position(|x| x.number == t.number) {
            Some(i) => tasks[i] = t,
            None => tasks.push(t),
        }
    }
    // Every open task stays; of the closed ones, the most recently updated.
    let (mut open, mut closed): (Vec<Task>, Vec<Task>) = tasks.into_iter().partition(|t| t.status.is_open());
    open.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    closed.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    closed.truncate(CLOSED_ISSUE_KEEP);
    open.extend(closed);

    let mut pulls = prev_pulls.to_vec();
    for p in delta.pulls {
        match pulls.iter().position(|x| x.number == p.number) {
            Some(i) => pulls[i] = p,
            None => pulls.push(p),
        }
    }
    pulls.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    pulls.truncate(PULL_KEEP);

    crate::github::Work {
        viewer: delta.viewer,
        tasks: open,
        pulls,
        labels: delta.labels,
        truncated,
        counts: delta.counts,
    }
}

#[derive(Clone)]
struct Session {
    login: String,
    source: AuthSource,
    github: GitHub,
    viewer: User,
}

#[derive(Default)]
struct ProjectState {
    /// ETags of the two "did anything change?" probes.
    etag_work: Option<String>,
    etag_repo: Option<String>,
    snapshot: Option<Snapshot>,
    members_at: Option<Instant>,
    /// When everything was last read in full (not just the changes).
    full_at: Option<Instant>,
    /// Set by our own writes. GitHub's change check keeps answering "not
    /// modified" for several seconds after a write (about 10 s when measured),
    /// so after writing we reload without asking it.
    dirty: bool,
    /// Label names known to exist in the repo (lowercase).
    labels: HashSet<String>,
}

pub struct Service {
    config_path: PathBuf,
    config: RwLock<Config>,
    /// Re-read the config file before every call. The MCP server runs as its
    /// own process, so this is how it sees projects added in the app.
    follow_config_file: bool,
    session: AsyncMutex<Option<Session>>,
    device: AsyncMutex<Option<DeviceSession>>,
    projects: Mutex<HashMap<String, ProjectState>>,
    /// Downloaded calendars by id, with when they were fetched.
    calendars: Mutex<HashMap<String, (Instant, Arc<calendar::Calendar>)>>,
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}

fn out<T: Serialize>(v: T) -> Result<Value> {
    Ok(serde_json::to_value(v)?)
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T> {
    let params = if params.is_null() { json!({}) } else { params };
    serde_json::from_value(params).map_err(|e| Error::invalid(format!("Bad parameters: {e}")))
}

/// Lets a field tell "not sent" (leave as is) from "sent as null" (clear it).
fn double_option<'de, T, D>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(d).map(Some)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ProjectParam {
    project: Option<String>,
    force: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LoginParam {
    login: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TokenParam {
    token: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct AddProject {
    repo: String,
    local_path: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct IdParam {
    id: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PathParam {
    id: String,
    path: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TaskRef {
    project: Option<String>,
    number: u64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TaskFilter {
    project: Option<String>,
    status: Option<String>,
    assignee: Option<String>,
    search: Option<String>,
    include_done: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TaskCreate {
    project: Option<String>,
    title: String,
    body: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    due: Option<String>,
    assignees: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TaskUpdate {
    project: Option<String>,
    number: u64,
    title: Option<String>,
    body: Option<String>,
    status: Option<String>,
    #[serde(deserialize_with = "double_option")]
    priority: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    due: Option<Option<String>>,
    assignees: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CommentAdd {
    project: Option<String>,
    number: u64,
    /// The issue's node id, when the caller already has it (saves a request).
    task_id: Option<String>,
    body: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CommentEdit {
    id: String,
    body: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct StartBranch {
    project: Option<String>,
    number: u64,
    name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PullLink {
    project: Option<String>,
    pull: u64,
    task: u64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PullCreate {
    project: Option<String>,
    title: String,
    head: String,
    base: Option<String>,
    body: Option<String>,
    draft: bool,
    task: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct GraphParam {
    project: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct BranchParam {
    project: Option<String>,
    branch: String,
    from: Option<String>,
    switch: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct UrlParam {
    url: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CalendarAdd {
    url: String,
    name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CalendarWindow {
    /// Start and end of the window as instants (RFC 3339).
    from: String,
    to: String,
    force: bool,
}

fn calendar_secret(id: &str) -> String {
    auth::calendar_account(id)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SettingsPatch {
    sync_seconds: Option<u32>,
    #[serde(deserialize_with = "double_option")]
    github_client_id: Option<Option<String>>,
    /// "en", "es", or null to follow the system language.
    #[serde(deserialize_with = "double_option")]
    language: Option<Option<String>>,
}

fn parse_status(s: &str) -> Result<Status> {
    Status::parse(s).ok_or_else(|| {
        Error::invalid(format!("Unknown status \"{s}\". Use todo, in_progress, in_review, done or canceled."))
    })
}

/// "" / "none" clear the priority.
fn parse_priority(s: &str) -> Result<Option<Priority>> {
    let t = s.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    Priority::parse(t)
        .map(Some)
        .ok_or_else(|| Error::invalid(format!("Unknown priority \"{s}\". Use urgent, high, medium, low or none.")))
}

/// "" clears the due date.
fn parse_due(s: &str) -> Result<Option<String>> {
    let t = s.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    if tasks::is_date(t) {
        Ok(Some(t.to_string()))
    } else {
        Err(Error::invalid(format!("\"{s}\" is not a date. Write it as YYYY-MM-DD, for example 2026-12-31.")))
    }
}

fn closing_keyword(task: u64) -> Regex {
    Regex::new(&format!(r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s*:?\s*#{task}\b")).expect("valid regex")
}

fn closing_line(task: u64) -> Regex {
    Regex::new(&format!(
        r"(?im)^[ \t]*(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s*:?\s*#{task}[ \t]*\r?\n?"
    ))
    .expect("valid regex")
}

fn cache_file(project_id: &str) -> PathBuf {
    let name = project_id.replace(['/', '\\'], "__").to_ascii_lowercase();
    config::cache_dir().join(format!("{name}.json"))
}

impl Service {
    pub fn new() -> Self {
        Self::with_config(config::config_path(), false)
    }

    /// For the MCP server: follows changes the desktop app makes to the config.
    pub fn following_config_file() -> Self {
        Self::with_config(config::config_path(), true)
    }

    pub fn with_config(path: PathBuf, follow_config_file: bool) -> Self {
        let config = Config::load(&path);
        Service {
            config_path: path,
            config: RwLock::new(config),
            follow_config_file,
            session: AsyncMutex::new(None),
            device: AsyncMutex::new(None),
            projects: Mutex::new(HashMap::new()),
            calendars: Mutex::new(HashMap::new()),
        }
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        if self.follow_config_file {
            let fresh = Config::load(&self.config_path);
            *self.config.write().await = fresh;
        }
        let result = self.dispatch(method, params).await;
        if matches!(result, Err(Error::Unauthorized)) {
            // The token may have been replaced (gh refreshes its own): read it again next time.
            *self.session.lock().await = None;
        }
        result
    }

    async fn dispatch(&self, method: &str, params: Value) -> Result<Value> {
        match method {
            "app_info" => self.app_info().await,
            "auth_status" => self.auth_status().await,
            "auth_use_gh" => self.auth_use_gh(parse(params)?).await,
            "auth_set_token" => self.auth_set_token(parse(params)?).await,
            "auth_device_start" => self.auth_device_start().await,
            "auth_device_poll" => self.auth_device_poll().await,
            "auth_sign_out" => self.auth_sign_out().await,
            "settings_get" => out(self.config.read().await.settings.clone()),
            "settings_set" => self.settings_set(parse(params)?).await,
            "projects_list" => self.projects_list().await,
            "projects_add" => self.projects_add(parse(params)?).await,
            "projects_remove" => self.projects_remove(parse(params)?).await,
            "projects_select" => self.projects_select(parse(params)?).await,
            "projects_set_path" => self.projects_set_path(parse(params)?).await,
            "repos_list" => out(self.session().await?.github.repos().await?),
            "local_inspect" => {
                let p: PathParam = parse(params)?;
                out(git::inspect(p.path.as_deref().unwrap_or("")).await?)
            }
            "snapshot" => self.snapshot(parse(params)?).await,
            "snapshot_cached" => self.snapshot_cached(parse(params)?).await,
            "sync" => self.sync(parse(params)?).await,
            "tasks_list" => self.tasks_list(parse(params)?).await,
            "task_get" => self.task_get(parse(params)?).await,
            "task_create" => self.task_create(parse(params)?).await,
            "task_update" => self.task_update(parse(params)?).await,
            "comment_add" => self.comment_add(parse(params)?).await,
            "comment_update" => {
                let p: CommentEdit = parse(params)?;
                if p.body.trim().is_empty() {
                    return Err(Error::invalid("A comment cannot be empty."));
                }
                out(self.session().await?.github.update_comment(&p.id, &p.body).await?)
            }
            "comment_delete" => {
                let p: CommentEdit = parse(params)?;
                self.session().await?.github.delete_comment(&p.id).await?;
                Ok(json!({ "deleted": true }))
            }
            "task_start_branch" => self.task_start_branch(parse(params)?).await,
            "pull_link" => self.pull_link(parse(params)?, true).await,
            "pull_unlink" => self.pull_link(parse(params)?, false).await,
            "pull_create" => self.pull_create(parse(params)?).await,
            "git_graph" => {
                let p: GraphParam = parse(params)?;
                let (project, path) = self.local(p.project.as_deref()).await?;
                let default = self.with_state(&project.id, |st| st.snapshot.as_ref().and_then(|s| s.repo.default_branch.clone()));
                out(git::graph(&path, p.limit.unwrap_or(git::DEFAULT_LIMIT), default.as_deref()).await?)
            }
            "git_status" => {
                let p: ProjectParam = parse(params)?;
                let (_, path) = self.local(p.project.as_deref()).await?;
                out(git::status(&path).await?)
            }
            "git_fetch" => {
                let p: ProjectParam = parse(params)?;
                let (_, path) = self.local(p.project.as_deref()).await?;
                git::fetch(&path).await?;
                Ok(json!({ "fetched": true }))
            }
            "git_switch" => {
                let p: BranchParam = parse(params)?;
                let (_, path) = self.local(p.project.as_deref()).await?;
                out(git::switch(&path, p.branch.trim()).await?)
            }
            "git_create_branch" => {
                let p: BranchParam = parse(params)?;
                let (_, path) = self.local(p.project.as_deref()).await?;
                out(git::create_branch(&path, p.branch.trim(), p.from.as_deref(), p.switch).await?)
            }
            "open_url" => {
                let p: UrlParam = parse(params)?;
                proc::open_url(&p.url)?;
                Ok(json!({ "opened": true }))
            }
            "calendar_list" => out(self.config.read().await.calendars.clone()),
            "calendar_add" => self.calendar_add(parse(params)?).await,
            "calendar_remove" => self.calendar_remove(parse(params)?).await,
            "calendar_events" => self.calendar_events(parse(params)?).await,
            _ => Err(Error::invalid(format!("Unknown method: {method}"))),
        }
    }

    // ---- plumbing --------------------------------------------------------

    fn with_state<R>(&self, project_id: &str, f: impl FnOnce(&mut ProjectState) -> R) -> R {
        let mut map = self.projects.lock().unwrap_or_else(|e| e.into_inner());
        f(map.entry(project_id.to_ascii_lowercase()).or_default())
    }

    /// Records that we changed something in the project on GitHub.
    fn wrote(&self, project: &Project) {
        self.with_state(&project.id, |st| st.dirty = true);
    }

    async fn project(&self, id: Option<&str>) -> Result<Project> {
        self.config.read().await.resolve(id).cloned()
    }

    /// The project and the folder of its local clone.
    async fn local(&self, id: Option<&str>) -> Result<(Project, String)> {
        let project = self.project(id).await?;
        match project.local_path.clone() {
            Some(path) => Ok((project, path)),
            None => Err(Error::invalid(format!(
                "{} has no local folder yet. Point Headway at your clone in Settings.",
                project.id
            ))),
        }
    }

    async fn save_config(&self, change: impl FnOnce(&mut Config)) -> Result<()> {
        let mut cfg = self.config.write().await;
        change(&mut cfg);
        cfg.save(&self.config_path)
    }

    async fn session(&self) -> Result<Session> {
        let auth = self.config.read().await.auth.clone().ok_or(Error::NotSignedIn)?;
        let mut guard = self.session.lock().await;
        if let Some(s) = guard.as_ref() {
            if s.login.eq_ignore_ascii_case(&auth.login) && s.source == auth.source {
                return Ok(s.clone());
            }
        }
        let token = match auth.source {
            AuthSource::Gh => auth::gh_token(&auth.login).await?,
            AuthSource::Keyring => auth::load_token(&auth.login)?.ok_or(Error::NotSignedIn)?,
        };
        let github = GitHub::new(&token)?;
        let viewer = github.viewer().await?;
        let s = Session { login: auth.login, source: auth.source, github, viewer };
        *guard = Some(s.clone());
        Ok(s)
    }

    async fn become_user(&self, source: AuthSource, github: GitHub, viewer: User) -> Result<Value> {
        let login = viewer.login.clone();
        *self.session.lock().await =
            Some(Session { login: login.clone(), source: source.clone(), github, viewer: viewer.clone() });
        self.projects.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.save_config(|c| c.auth = Some(AuthConfig { source, login })).await?;
        Ok(json!({ "signedIn": true, "user": viewer }))
    }

    // ---- app, auth, settings --------------------------------------------

    async fn app_info(&self) -> Result<Value> {
        // The MCP server is this same program started with `--mcp`. Only the
        // real app can say where it is (not the dev server or a test).
        let exe = std::env::current_exe().ok().filter(|e| {
            e.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("headway"))
        });
        Ok(json!({
            "name": crate::APP_NAME,
            "version": env!("CARGO_PKG_VERSION"),
            "configPath": self.config_path.to_string_lossy(),
            "exePath": exe.map(|p| p.to_string_lossy().to_string()),
        }))
    }

    async fn auth_status(&self) -> Result<Value> {
        let cfg = self.config.read().await.clone();
        let gh_accounts = auth::gh_accounts().await;
        let mut status = json!({
            "signedIn": false,
            "ghAccounts": gh_accounts,
            "deviceFlowConfigured": cfg.settings.github_client_id.as_deref().is_some_and(|s| !s.trim().is_empty()),
        });
        if let Some(a) = &cfg.auth {
            status["login"] = json!(a.login);
            status["source"] = json!(a.source);
            match self.session().await {
                Ok(s) => {
                    status["signedIn"] = json!(true);
                    status["user"] = json!(s.viewer);
                }
                Err(e) => status["error"] = json!(e),
            }
        }
        Ok(status)
    }

    async fn auth_use_gh(&self, p: LoginParam) -> Result<Value> {
        let token = auth::gh_token(p.login.trim()).await?;
        let github = GitHub::new(&token)?;
        let viewer = github.viewer().await?;
        self.become_user(AuthSource::Gh, github, viewer).await
    }

    async fn auth_set_token(&self, p: TokenParam) -> Result<Value> {
        let token = p.token.trim();
        if token.is_empty() {
            return Err(Error::invalid("Paste a GitHub token first."));
        }
        let github = GitHub::new(token)?;
        let viewer = github.viewer().await?;
        auth::store_token(&viewer.login, token)?;
        self.become_user(AuthSource::Keyring, github, viewer).await
    }

    async fn auth_device_start(&self) -> Result<Value> {
        let client_id = self.config.read().await.settings.github_client_id.clone().unwrap_or_default();
        let session = auth::device_start(&client_id).await?;
        let info = json!({
            "userCode": session.user_code,
            "verificationUri": session.verification_uri,
            "interval": session.interval,
            "expiresIn": session.expires_at.saturating_duration_since(Instant::now()).as_secs(),
        });
        *self.device.lock().await = Some(session);
        Ok(info)
    }

    async fn auth_device_poll(&self) -> Result<Value> {
        let mut guard = self.device.lock().await;
        let Some(session) = guard.as_mut() else {
            return Ok(json!({ "status": "expired" }));
        };
        match auth::device_poll(session).await? {
            DevicePoll::Pending => Ok(json!({ "status": "pending", "interval": session.interval })),
            DevicePoll::SlowDown => {
                session.interval += 5;
                Ok(json!({ "status": "pending", "interval": session.interval }))
            }
            DevicePoll::Expired => {
                *guard = None;
                Ok(json!({ "status": "expired" }))
            }
            DevicePoll::Denied => {
                *guard = None;
                Ok(json!({ "status": "denied" }))
            }
            DevicePoll::Token(token) => {
                *guard = None;
                drop(guard);
                let github = GitHub::new(&token)?;
                let viewer = github.viewer().await?;
                auth::store_token(&viewer.login, &token)?;
                let mut done = self.become_user(AuthSource::Keyring, github, viewer).await?;
                done["status"] = json!("ok");
                Ok(done)
            }
        }
    }

    async fn auth_sign_out(&self) -> Result<Value> {
        let auth = self.config.read().await.auth.clone();
        if let Some(a) = auth {
            if a.source == AuthSource::Keyring {
                auth::delete_token(&a.login)?;
            }
        }
        *self.session.lock().await = None;
        self.projects.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.save_config(|c| c.auth = None).await?;
        Ok(json!({ "signedIn": false }))
    }

    async fn settings_set(&self, p: SettingsPatch) -> Result<Value> {
        if let Some(Some(lang)) = &p.language {
            if !config::LANGUAGES.contains(&lang.as_str()) {
                return Err(Error::invalid(format!("Headway is not available in \"{lang}\". Use en or es.")));
            }
        }
        self.save_config(|c| {
            if let Some(lang) = p.language {
                c.settings.language = lang;
            }
            if let Some(s) = p.sync_seconds {
                c.settings.sync_seconds = s.clamp(10, 3600);
            }
            if let Some(id) = p.github_client_id {
                c.settings.github_client_id = id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            }
        })
        .await?;
        out(self.config.read().await.settings.clone())
    }

    // ---- calendars ---------------------------------------------------------
    //
    // A calendar is connected by its private link (Google Calendar: "Secret
    // address in iCal format"). The link stays in the OS credential store; the
    // UI only ever sees the calendar's id and name.

    async fn calendar_add(&self, p: CalendarAdd) -> Result<Value> {
        let url = calendar::normalize_url(&p.url)?;
        let existing = self.config.read().await.calendars.clone();
        for c in &existing {
            if auth::load_secret(&calendar_secret(&c.id))?.as_deref() == Some(url.as_str()) {
                return Err(Error::invalid(format!("That calendar is already connected as \"{}\".", c.name)));
            }
        }
        // Download it once now, so a wrong address is refused instead of saved.
        let cal = calendar::parse(&calendar::download(&url).await?)?;
        let name = p
            .name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .or_else(|| cal.name.clone())
            .unwrap_or_else(|| "Calendar".to_string());
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let entry = CalendarRef { id: format!("{nanos:x}"), name, added_at: chrono::Utc::now().to_rfc3339() };
        auth::store_secret(&calendar_secret(&entry.id), &url)?;
        let events = cal.event_count();
        self.calendars.lock().unwrap_or_else(|e| e.into_inner()).insert(entry.id.clone(), (Instant::now(), Arc::new(cal)));
        self.save_config(|c| c.calendars.push(entry.clone())).await?;
        Ok(json!({ "calendar": entry, "events": events }))
    }

    async fn calendar_remove(&self, p: IdParam) -> Result<Value> {
        auth::delete_secret(&calendar_secret(&p.id))?;
        self.calendars.lock().unwrap_or_else(|e| e.into_inner()).remove(&p.id);
        self.save_config(|c| c.calendars.retain(|x| x.id != p.id)).await?;
        out(self.config.read().await.calendars.clone())
    }

    /// The parsed calendar, downloaded again when the copy in memory is old.
    async fn calendar_data(&self, id: &str, force: bool) -> Result<Arc<calendar::Calendar>> {
        let cached = self.calendars.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned();
        if let (Some((at, cal)), false) = (&cached, force) {
            if at.elapsed() < CALENDAR_REFRESH {
                return Ok(cal.clone());
            }
        }
        let url = auth::load_secret(&calendar_secret(id))?.ok_or_else(|| {
            Error::invalid("This calendar's address is no longer stored on this computer. Remove it and connect it again.")
        })?;
        let cal = Arc::new(calendar::parse(&calendar::download(&url).await?)?);
        self.calendars.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), (Instant::now(), cal.clone()));
        Ok(cal)
    }

    async fn calendar_events(&self, p: CalendarWindow) -> Result<Value> {
        let instant = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s.trim())
                .map(|d| d.with_timezone(&chrono::Utc))
                .map_err(|_| Error::invalid("The agenda window needs a start and an end time."))
        };
        let (from, to) = (instant(&p.from)?, instant(&p.to)?);
        if to <= from || (to - from).num_days() > CALENDAR_MAX_DAYS {
            return Err(Error::invalid("The agenda window must be between one moment and about three months long."));
        }
        let list = self.config.read().await.calendars.clone();
        let mut events: Vec<Value> = Vec::new();
        let mut problems: Vec<Value> = Vec::new();
        for c in &list {
            let cal = match self.calendar_data(&c.id, p.force).await {
                Ok(cal) => cal,
                Err(e) => {
                    // Keep showing the last copy we have rather than emptying the agenda.
                    let stale = self.calendars.lock().unwrap_or_else(|x| x.into_inner()).get(&c.id).map(|(_, cal)| cal.clone());
                    problems.push(json!({ "calendarId": c.id, "calendar": c.name, "message": e.to_string(), "showingOldCopy": stale.is_some() }));
                    match stale {
                        Some(cal) => cal,
                        None => continue,
                    }
                }
            };
            for o in calendar::occurrences(&cal, from, to) {
                let mut v = serde_json::to_value(&o)?;
                v["calendarId"] = json!(c.id);
                v["calendar"] = json!(c.name);
                events.push(v);
            }
        }
        let key = |v: &Value| {
            let start = v["start"].as_str().unwrap_or("").to_string();
            (start.get(..10).unwrap_or("").to_string(), !v["allDay"].as_bool().unwrap_or(false), start)
        };
        events.sort_by_key(key);
        Ok(json!({ "calendars": list, "events": events, "problems": problems }))
    }

    // ---- projects --------------------------------------------------------

    async fn projects_list(&self) -> Result<Value> {
        let cfg = self.config.read().await;
        Ok(json!({ "projects": cfg.projects, "currentId": cfg.current().map(|p| p.id.clone()) }))
    }

    async fn projects_add(&self, p: AddProject) -> Result<Value> {
        let (owner, name) = config::parse_repo(&p.repo).ok_or_else(|| {
            Error::invalid("Write the repository as owner/name, or paste its GitHub address.")
        })?;
        let s = self.session().await?;
        let repo = s.github.repo(&owner, &name).await?;
        // GitHub's own spelling (it is case-insensitive about what we asked for).
        let id = repo.name_with_owner.clone();
        let (owner, name) = id.split_once('/').map(|(o, n)| (o.to_string(), n.to_string())).unwrap_or((owner, name));

        let mut local_path = None;
        if let Some(path) = p.local_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            local_path = Some(self.check_clone(path, &id).await?);
        }
        let mut project = Project { id: id.clone(), owner, repo: name, local_path, added_at: chrono::Utc::now().to_rfc3339() };
        self.save_config(|c| {
            match c.project_mut(&id) {
                Some(existing) => {
                    if project.local_path.is_some() {
                        existing.local_path = project.local_path.clone();
                    }
                    project = existing.clone();
                }
                None => c.projects.push(project.clone()),
            }
            c.current_project = Some(id.clone());
        })
        .await?;
        Ok(json!({ "project": project, "repo": repo }))
    }

    /// Confirms `path` is a clone of the project's repository; returns its root.
    async fn check_clone(&self, path: &str, repo_id: &str) -> Result<String> {
        let local = git::inspect(path).await?;
        if !local.is_clone_of(repo_id) {
            let found: Vec<String> = local.remotes.iter().filter_map(|r| r.repo.clone()).collect();
            return Err(Error::invalid(if found.is_empty() {
                format!("That folder is a git repository, but it has no remote pointing at {repo_id}.")
            } else {
                format!("That folder is a clone of {}, not of {repo_id}.", found.join(", "))
            }));
        }
        Ok(local.root)
    }

    async fn projects_remove(&self, p: IdParam) -> Result<Value> {
        self.save_config(|c| {
            c.projects.retain(|x| !x.id.eq_ignore_ascii_case(&p.id));
            if c.current_project.as_deref().is_some_and(|cur| cur.eq_ignore_ascii_case(&p.id)) {
                c.current_project = c.projects.first().map(|x| x.id.clone());
            }
        })
        .await?;
        let _ = std::fs::remove_file(cache_file(&p.id));
        self.projects_list().await
    }

    async fn projects_select(&self, p: IdParam) -> Result<Value> {
        let project = self.project(Some(&p.id)).await?;
        self.save_config(|c| c.current_project = Some(project.id.clone())).await?;
        self.projects_list().await
    }

    async fn projects_set_path(&self, p: PathParam) -> Result<Value> {
        let project = self.project(Some(&p.id)).await?;
        let root = match p.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(path) => Some(self.check_clone(path, &project.id).await?),
            None => None,
        };
        self.save_config(|c| {
            if let Some(x) = c.project_mut(&project.id) {
                x.local_path = root.clone();
            }
        })
        .await?;
        self.projects_list().await
    }

    // ---- reading ---------------------------------------------------------

    /// Brings the project's snapshot up to date and says whether it changed.
    ///
    /// Two conditional requests ask GitHub "did any task or pull request
    /// change?" and "did anyone push?". Both are free when the answer is no
    /// (304). Only the parts that moved are then read again.
    async fn refresh(&self, project: &Project, force: bool) -> Result<(bool, Snapshot)> {
        let s = self.session().await?;
        let (owner, name) = (project.owner.as_str(), project.repo.as_str());
        let (etag_work, etag_repo, previous, members_stale, dirty) = self.with_state(&project.id, |st| {
            (
                st.etag_work.clone(),
                st.etag_repo.clone(),
                st.snapshot.clone(),
                st.members_at.is_none_or(|t| t.elapsed() >= MEMBERS_REFRESH),
                std::mem::take(&mut st.dirty),
            )
        });
        let work_probe = s.github.probe(&GitHub::work_probe_path(owner, name), etag_work.as_deref()).await?;
        let repo_probe = s.github.probe(&GitHub::repo_probe_path(owner, name), etag_repo.as_deref()).await?;

        let work_due = force || dirty || previous.is_none() || matches!(work_probe, Probe::Changed(_));
        // A merged pull request moves both the tasks and the commit totals.
        let repo_due = work_due || matches!(repo_probe, Probe::Changed(_));
        let members_due = force || previous.is_none() || members_stale;
        if let (Some(snap), false, false, false) = (&previous, work_due, repo_due, members_due) {
            return Ok((false, snap.clone()));
        }

        // After the first full read, only what changed is fetched and merged in.
        // A full read still happens now and then, on "Refresh", and at start-up.
        let since = previous.as_ref().and_then(|p| since_with_margin(&p.fetched_at));
        let full_stale = self.with_state(&project.id, |st| st.full_at.is_none_or(|t| t.elapsed() >= FULL_RELOAD));
        let work = if !work_due {
            None
        } else if let (Some(prev), Some(since), false, false) = (&previous, &since, force, full_stale) {
            let delta = s.github.work_since(owner, name, since).await?;
            Some(merge_work(&prev.tasks, &prev.pulls, prev.tasks_truncated, delta))
        } else {
            let all = s.github.work(owner, name).await?;
            self.with_state(&project.id, |st| st.full_at = Some(Instant::now()));
            Some(all)
        };
        let mut repo = match (&previous, repo_due) {
            (Some(prev), false) => prev.repo.clone(),
            _ => s.github.repo(owner, name).await?,
        };
        let members = match (&previous, members_due) {
            (Some(prev), false) => prev.members.clone(),
            _ => {
                let m = s.github.members(owner, name).await?;
                self.with_state(&project.id, |st| st.members_at = Some(Instant::now()));
                m
            }
        };

        let fetched_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let snap = match (work, previous) {
            (Some(work), _) => {
                // Totals that move with every task change: take them from the same read.
                repo.open_issues = work.counts.open_issues;
                repo.closed_issues = work.counts.closed_issues;
                repo.open_pulls = work.counts.open_pulls;
                repo.merged_pulls = work.counts.merged_pulls;
                repo.closed_pulls = work.counts.closed_pulls;
                Snapshot {
                    project_id: project.id.clone(),
                    fetched_at,
                    viewer: work.viewer,
                    repo,
                    tasks: work.tasks,
                    pulls: work.pulls,
                    members,
                    labels: work.labels,
                    tasks_truncated: work.truncated,
                    from_cache: false,
                }
            }
            (None, Some(prev)) => Snapshot { fetched_at, repo, members, from_cache: false, ..prev },
            (None, None) => unreachable!("without a previous snapshot the work is always read"),
        };

        self.with_state(&project.id, |st| {
            st.labels = snap.labels.iter().map(|l| l.name.to_lowercase()).collect();
            st.snapshot = Some(snap.clone());
            // The ETags were taken before reading, so a change that slipped in
            // between only causes one extra reload next time, never a missed one.
            if let Probe::Changed(tag) = work_probe {
                st.etag_work = tag;
            }
            if let Probe::Changed(tag) = repo_probe {
                st.etag_repo = tag;
            }
        });
        if let Ok(text) = serde_json::to_string(&snap) {
            let file = cache_file(&project.id);
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(file, text);
        }
        Ok((true, snap))
    }

    /// The project's current data. `force` re-reads everything from GitHub;
    /// without it only what changed since the last call is read.
    async fn snapshot(&self, p: ProjectParam) -> Result<Value> {
        let project = self.project(p.project.as_deref()).await?;
        out(self.refresh(&project, p.force).await?.1)
    }

    /// The last snapshot saved on disk, for an instant first paint.
    async fn snapshot_cached(&self, p: ProjectParam) -> Result<Value> {
        let project = self.project(p.project.as_deref()).await?;
        let Ok(text) = std::fs::read_to_string(cache_file(&project.id)) else { return Ok(Value::Null) };
        match serde_json::from_str::<Snapshot>(&text) {
            Ok(mut snap) => {
                snap.from_cache = true;
                out(snap)
            }
            Err(_) => Ok(Value::Null),
        }
    }

    async fn sync(&self, p: ProjectParam) -> Result<Value> {
        let project = self.project(p.project.as_deref()).await?;
        let (changed, snap) = self.refresh(&project, p.force).await?;
        if changed { Ok(json!({ "changed": true, "snapshot": snap })) } else { Ok(json!({ "changed": false })) }
    }

    async fn tasks_list(&self, p: TaskFilter) -> Result<Value> {
        let project = self.project(p.project.as_deref()).await?;
        let (_, snap) = self.refresh(&project, false).await?;
        let status = p.status.as_deref().map(parse_status).transpose()?;
        let search = p.search.as_deref().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
        let assignee = p.assignee.as_deref().map(|a| a.trim().trim_start_matches('@').to_lowercase()).filter(|s| !s.is_empty());
        let me = snap.viewer.login.to_lowercase();
        let list: Vec<&Task> = snap
            .tasks
            .iter()
            .filter(|t| match status {
                Some(s) => t.status == s,
                None => p.include_done || t.status.is_open(),
            })
            .filter(|t| match assignee.as_deref() {
                None => true,
                Some("none") | Some("unassigned") => t.assignees.is_empty(),
                Some(a) => {
                    let who = if a == "me" { me.as_str() } else { a };
                    t.assignees.iter().any(|u| u.login.to_lowercase() == who)
                }
            })
            .filter(|t| match search.as_deref() {
                None => true,
                Some(q) => t.title.to_lowercase().contains(q) || t.body.to_lowercase().contains(q),
            })
            .collect();
        Ok(json!({ "project": project.id, "tasks": list, "truncated": snap.tasks_truncated }))
    }

    async fn task_get(&self, p: TaskRef) -> Result<Value> {
        let project = self.project(p.project.as_deref()).await?;
        let s = self.session().await?;
        out(s.github.task(&project.owner, &project.repo, p.number).await?)
    }

    // ---- writing ---------------------------------------------------------

    /// Creates Headway's labels in the repo if they are missing.
    async fn ensure_labels(&self, s: &Session, project: &Project, on_issue: &[String], wanted: &[String]) -> Result<()> {
        let mut have: Vec<String> = self.with_state(&project.id, |st| st.labels.iter().cloned().collect());
        have.extend(on_issue.iter().cloned());
        s.github.ensure_labels(&project.owner, &project.repo, &have, wanted).await?;
        self.with_state(&project.id, |st| st.labels.extend(wanted.iter().map(|w| w.to_lowercase())));
        Ok(())
    }

    async fn task_create(&self, p: TaskCreate) -> Result<Value> {
        let title = p.title.trim();
        if title.is_empty() {
            return Err(Error::invalid("A task needs a title."));
        }
        let status = p.status.as_deref().map(parse_status).transpose()?.unwrap_or(Status::Todo);
        let priority = p.priority.as_deref().map(parse_priority).transpose()?.flatten();
        let due = p.due.as_deref().map(parse_due).transpose()?.flatten();

        let project = self.project(p.project.as_deref()).await?;
        let s = self.session().await?;
        let none: [&str; 0] = [];
        let labels = tasks::apply_labels(&none, Some(status), Some(priority));
        self.ensure_labels(&s, &project, &[], &labels).await?;

        let mut meta = tasks::Meta::default();
        meta.set_due(due.as_deref());
        let body = tasks::join_body(p.body.as_deref().unwrap_or(""), &meta);
        let assignees: Vec<String> = p.assignees.iter().map(|a| a.trim().trim_start_matches('@').to_string()).filter(|a| !a.is_empty()).collect();
        let number = s.github.create_issue(&project.owner, &project.repo, title, &body, &assignees, &labels).await?;
        self.wrote(&project);
        if !status.is_open() {
            let reason = if status == Status::Canceled { "not_planned" } else { "completed" };
            s.github
                .patch_issue(&project.owner, &project.repo, number, json!({ "state": "closed", "state_reason": reason }))
                .await?;
        }
        out(s.github.task(&project.owner, &project.repo, number).await?)
    }

    async fn task_update(&self, p: TaskUpdate) -> Result<Value> {
        if p.number == 0 {
            return Err(Error::invalid("Which task? Give its number."));
        }
        let status = p.status.as_deref().map(parse_status).transpose()?;
        let priority = match &p.priority {
            None => None,
            Some(None) => Some(None),
            Some(Some(s)) => Some(parse_priority(s)?),
        };
        let due = match &p.due {
            None => None,
            Some(None) => Some(None),
            Some(Some(s)) => Some(parse_due(s)?),
        };

        let project = self.project(p.project.as_deref()).await?;
        let s = self.session().await?;
        let (owner, name) = (project.owner.as_str(), project.repo.as_str());
        // Read the issue as it is right now, so labels and text other people
        // changed meanwhile are kept.
        let raw = s.github.issue_raw(owner, name, p.number).await?;

        let mut patch = Map::new();
        if let Some(t) = p.title.as_deref() {
            if t.trim().is_empty() {
                return Err(Error::invalid("A task needs a title."));
            }
            patch.insert("title".into(), json!(t.trim()));
        }
        if p.body.is_some() || due.is_some() {
            let (old_text, mut meta) = tasks::split_body(&raw.body);
            if let Some(d) = &due {
                meta.set_due(d.as_deref());
            }
            patch.insert("body".into(), json!(tasks::join_body(p.body.as_deref().unwrap_or(&old_text), &meta)));
        }
        if status.is_some() || priority.is_some() {
            let labels = tasks::apply_labels(&raw.labels, status, priority);
            self.ensure_labels(&s, &project, &raw.labels, &labels).await?;
            patch.insert("labels".into(), json!(labels));
        }
        match status {
            Some(Status::Done) => {
                patch.insert("state".into(), json!("closed"));
                patch.insert("state_reason".into(), json!("completed"));
            }
            Some(Status::Canceled) => {
                patch.insert("state".into(), json!("closed"));
                patch.insert("state_reason".into(), json!("not_planned"));
            }
            Some(_) if raw.closed => {
                patch.insert("state".into(), json!("open"));
                patch.insert("state_reason".into(), json!("reopened"));
            }
            _ => {}
        }
        if let Some(a) = &p.assignees {
            let logins: Vec<&str> = a.iter().map(|x| x.trim().trim_start_matches('@')).filter(|x| !x.is_empty()).collect();
            patch.insert("assignees".into(), json!(logins));
        }
        if !patch.is_empty() {
            s.github.patch_issue(owner, name, p.number, Value::Object(patch)).await?;
            self.wrote(&project);
        }
        out(s.github.task(owner, name, p.number).await?)
    }

    async fn comment_add(&self, p: CommentAdd) -> Result<Value> {
        if p.body.trim().is_empty() {
            return Err(Error::invalid("A comment cannot be empty."));
        }
        let project = self.project(p.project.as_deref()).await?;
        let s = self.session().await?;
        let subject = match p.task_id.filter(|id| !id.is_empty()) {
            Some(id) => id,
            None => s.github.issue_raw(&project.owner, &project.repo, p.number).await?.id,
        };
        let comment = s.github.add_comment(&subject, &p.body).await?;
        self.wrote(&project);
        out(comment)
    }

    /// Creates a branch on GitHub linked to the task: a pull request opened
    /// from it attaches to the task by itself.
    async fn task_start_branch(&self, p: StartBranch) -> Result<Value> {
        let project = self.project(p.project.as_deref()).await?;
        let s = self.session().await?;
        let (owner, name) = (project.owner.as_str(), project.repo.as_str());
        let raw = s.github.issue_raw(owner, name, p.number).await?;
        let branch = match p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => n.to_string(),
            None => tasks::branch_name(p.number, &raw.title),
        };
        let (repo_id, base, oid) = s.github.branch_base(owner, name).await?;
        let created = s.github.create_linked_branch(&raw.id, &repo_id, &oid, &branch).await?;
        self.wrote(&project);
        Ok(json!({ "branch": created, "base": base, "url": format!("https://github.com/{owner}/{name}/tree/{created}") }))
    }

    /// Links (or unlinks) a pull request and a task the way GitHub documents
    /// it: a "Closes #N" line in the pull request's description.
    async fn pull_link(&self, p: PullLink, link: bool) -> Result<Value> {
        if p.pull == 0 || p.task == 0 {
            return Err(Error::invalid("Give both the pull request number and the task number."));
        }
        let project = self.project(p.project.as_deref()).await?;
        let s = self.session().await?;
        let (owner, name) = (project.owner.as_str(), project.repo.as_str());
        let (body, base) = s.github.pull_body(owner, name, p.pull).await?;
        let default = self.with_state(&project.id, |st| st.snapshot.as_ref().and_then(|x| x.repo.default_branch.clone()));
        let already = closing_keyword(p.task).is_match(&body);

        if link {
            if !already {
                let text = body.trim_end();
                let new_body = if text.is_empty() { format!("Closes #{}", p.task) } else { format!("{text}\n\nCloses #{}", p.task) };
                s.github.set_pull_body(owner, name, p.pull, &new_body).await?;
                self.wrote(&project);
            }
            // GitHub only acts on the keyword when the pull request targets the default branch.
            let note = match default {
                Some(d) if d != base => Some(format!(
                    // i18n
                    "This pull request targets \"{base}\". GitHub only closes the task automatically for pull requests into \"{d}\"."
                )),
                _ => None,
            };
            return Ok(json!({ "linked": true, "changed": !already, "note": note }));
        }

        if !already {
            return Err(Error::invalid(format!(
                "Pull request #{} has no \"Closes #{}\" in its description. If the link was made in GitHub's sidebar, remove it there.",
                p.pull, p.task
            )));
        }
        let new_body = closing_line(p.task).replace_all(&body, "").trim_end().to_string();
        if closing_keyword(p.task).is_match(&new_body) {
            return Err(Error::invalid(
                "The link is written inside a sentence of the description. Edit the pull request on GitHub to remove it.",
            ));
        }
        s.github.set_pull_body(owner, name, p.pull, &new_body).await?;
        self.wrote(&project);
        Ok(json!({ "linked": false, "changed": true }))
    }

    async fn pull_create(&self, p: PullCreate) -> Result<Value> {
        if p.title.trim().is_empty() || p.head.trim().is_empty() {
            return Err(Error::invalid("A pull request needs a title and the branch to merge."));
        }
        let project = self.project(p.project.as_deref()).await?;
        let s = self.session().await?;
        let (owner, name) = (project.owner.as_str(), project.repo.as_str());
        let base = match p.base.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            Some(b) => b.to_string(),
            None => s.github.branch_base(owner, name).await?.1,
        };
        let mut body = p.body.unwrap_or_default().trim_end().to_string();
        if let Some(task) = p.task {
            if !closing_keyword(task).is_match(&body) {
                if !body.is_empty() {
                    body.push_str("\n\n");
                }
                body.push_str(&format!("Closes #{task}"));
            }
        }
        let (number, url) = s.github.create_pull(owner, name, p.title.trim(), p.head.trim(), &base, &body, p.draft).await?;
        self.wrote(&project);
        Ok(json!({ "number": number, "url": url, "base": base }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_closing_keywords_for_the_right_task_only() {
        let re = closing_keyword(12);
        assert!(re.is_match("Closes #12"));
        assert!(re.is_match("this fixes #12."));
        assert!(re.is_match("Resolved: #12"));
        assert!(!re.is_match("Closes #123"));
        assert!(!re.is_match("See #12"));
        assert!(!re.is_match("Closes #1"));
    }

    #[test]
    fn removes_only_whole_closing_lines() {
        let body = "Adds the login page.\n\nCloses #12\nCloses #13\n";
        let left = closing_line(12).replace_all(body, "").to_string();
        assert_eq!(left, "Adds the login page.\n\nCloses #13\n");
        let inline = "This PR fixes #12 and more.";
        assert_eq!(closing_line(12).replace_all(inline, ""), inline);
    }

    fn task(number: u64, status: &str, updated: &str) -> Task {
        serde_json::from_value(json!({
            "id": format!("I{number}"), "number": number, "title": format!("task {number}"), "body": "", "url": "",
            "status": status, "priority": null, "due": null, "assignees": [], "author": null, "labels": [],
            "commentCount": 0, "createdAt": "2026-01-01T00:00:00Z", "updatedAt": updated, "closedAt": null,
            "pulls": [], "branches": []
        }))
        .unwrap()
    }

    #[test]
    fn changes_are_merged_into_the_previous_snapshot() {
        let prev = vec![
            task(1, "todo", "2026-11-20T10:00:00Z"),
            task(2, "in_progress", "2026-11-21T10:00:00Z"),
            task(3, "done", "2026-11-19T10:00:00Z"),
        ];
        let viewer = User { login: "me".into(), name: None, avatar_url: String::new(), url: String::new() };
        let delta = crate::github::Work {
            viewer,
            // #2 was finished, #4 is new.
            tasks: vec![task(2, "done", "2026-11-25T09:00:00Z"), task(4, "todo", "2026-11-25T08:00:00Z")],
            pulls: vec![],
            labels: vec![],
            truncated: false,
            counts: Default::default(),
        };
        let merged = merge_work(&prev, &[], true, delta);
        let seen: Vec<(u64, Status)> = merged.tasks.iter().map(|t| (t.number, t.status)).collect();
        // Open tasks first (newest change first), then closed ones (newest change first).
        assert_eq!(seen, vec![(4, Status::Todo), (1, Status::Todo), (2, Status::Done), (3, Status::Done)]);
        assert!(merged.truncated);
    }

    #[test]
    fn asks_for_changes_from_a_little_before_the_last_read() {
        assert_eq!(since_with_margin("2026-11-25T17:10:44Z").as_deref(), Some("2026-11-25T17:05:44Z"));
        assert_eq!(since_with_margin("2026-11-25T14:10:44-03:00").as_deref(), Some("2026-11-25T17:05:44Z"));
        assert_eq!(since_with_margin("yesterday"), None);
    }

    #[test]
    fn update_params_tell_missing_from_null() {
        let p: TaskUpdate = parse(json!({ "number": 3, "priority": null, "status": "done" })).unwrap();
        assert_eq!(p.priority, Some(None));
        assert_eq!(p.due, None);
        assert_eq!(p.status.as_deref(), Some("done"));
        let p: TaskUpdate = parse(json!({ "number": 3, "due": "2026-12-01" })).unwrap();
        assert_eq!(p.due, Some(Some("2026-12-01".into())));
        assert_eq!(p.priority, None);
    }

    #[test]
    fn validates_field_values() {
        assert!(parse_status("In Progress").is_ok());
        assert!(parse_status("someday").is_err());
        assert_eq!(parse_priority("none").unwrap(), None);
        assert_eq!(parse_priority("High").unwrap(), Some(Priority::High));
        assert!(parse_priority("meh").is_err());
        assert_eq!(parse_due("").unwrap(), None);
        assert_eq!(parse_due("2026-12-31").unwrap().as_deref(), Some("2026-12-31"));
        assert!(parse_due("31/12/2026").is_err());
        assert!(parse_due("2026-02-30").is_err());
    }

    #[tokio::test]
    async fn unknown_methods_and_missing_sign_in_are_reported() {
        let dir = std::env::temp_dir().join(format!("headway-svc-{}", std::process::id()));
        let svc = Service::with_config(dir.join("config.json"), false);
        assert_eq!(svc.call("nope", Value::Null).await.unwrap_err().code(), "invalid");
        assert_eq!(svc.call("repos_list", Value::Null).await.unwrap_err().code(), "not_signed_in");
        assert_eq!(svc.call("snapshot", Value::Null).await.unwrap_err().code(), "not_found");
        let list = svc.call("projects_list", Value::Null).await.unwrap();
        assert_eq!(list["projects"].as_array().unwrap().len(), 0);
        assert_eq!(svc.call("open_url", json!({ "url": "file:///c:/windows" })).await.unwrap_err().code(), "invalid");
    }
}
