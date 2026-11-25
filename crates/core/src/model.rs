//! The shapes the UI and the MCP server see. Everything here serializes as
//! camelCase JSON.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub login: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub url: String,
}

/// Where a task stands. On GitHub: an open issue is `Todo` unless it carries a
/// status label; a closed issue is `Done`, or `Canceled` when closed as
/// "not planned".
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Todo,
    InProgress,
    InReview,
    Done,
    Canceled,
}

impl Status {
    pub const ALL: [Status; 5] = [
        Status::Todo,
        Status::InProgress,
        Status::InReview,
        Status::Done,
        Status::Canceled,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Todo => "todo",
            Status::InProgress => "in_progress",
            Status::InReview => "in_review",
            Status::Done => "done",
            Status::Canceled => "canceled",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Status::Todo => "To do",
            Status::InProgress => "In progress",
            Status::InReview => "In review",
            Status::Done => "Done",
            Status::Canceled => "Canceled",
        }
    }

    pub fn is_open(self) -> bool {
        !matches!(self, Status::Done | Status::Canceled)
    }

    /// Accepts "in_progress", "in progress", "In-Progress", "inprogress" ...
    pub fn parse(s: &str) -> Option<Status> {
        let k: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        match k.as_str() {
            "todo" | "open" | "backlog" => Some(Status::Todo),
            "inprogress" | "doing" | "started" | "wip" => Some(Status::InProgress),
            "inreview" | "review" => Some(Status::InReview),
            "done" | "closed" | "complete" | "completed" => Some(Status::Done),
            "canceled" | "cancelled" | "wontdo" | "notplanned" => Some(Status::Canceled),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Urgent,
    High,
    Medium,
    Low,
}

impl Priority {
    pub const ALL: [Priority; 4] = [
        Priority::Urgent,
        Priority::High,
        Priority::Medium,
        Priority::Low,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Urgent => "urgent",
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }

    pub fn parse(s: &str) -> Option<Priority> {
        match s.trim().to_ascii_lowercase().as_str() {
            "urgent" | "critical" | "p0" => Some(Priority::Urgent),
            "high" | "p1" => Some(Priority::High),
            "medium" | "normal" | "p2" => Some(Priority::Medium),
            "low" | "p3" => Some(Priority::Low),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub name: String,
    /// Hex without the leading '#', as GitHub returns it.
    #[serde(default)]
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PullState {
    Open,
    Merged,
    Closed,
}

/// A pull request as seen from a task (just enough to draw a chip).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PullRef {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PullState,
    pub is_draft: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// GitHub node id of the issue.
    pub id: String,
    /// Issue number: the id people use ("#12").
    pub number: u64,
    pub title: String,
    /// Markdown description, without Headway's hidden metadata comment.
    pub body: String,
    /// Description rendered by GitHub. Only present on a single-task fetch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_html: Option<String>,
    pub url: String,
    pub status: Status,
    pub priority: Option<Priority>,
    /// Due date as YYYY-MM-DD.
    pub due: Option<String>,
    pub assignees: Vec<User>,
    pub author: Option<User>,
    /// Labels that are not status/priority labels.
    pub labels: Vec<Label>,
    pub comment_count: u32,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    /// Pull requests GitHub lists as closing this issue.
    pub pulls: Vec<PullRef>,
    /// Branches created for this issue ("linked branches").
    pub branches: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: Option<User>,
    pub body: String,
    pub body_html: String,
    pub created_at: String,
    pub updated_at: String,
    pub url: String,
    pub can_edit: bool,
    pub can_delete: bool,
    pub is_mine: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    #[serde(flatten)]
    pub task: Task,
    pub comments: Vec<Comment>,
}

/// CI summary of a pull request's last commit.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    /// success | failure | pending | error | expected
    pub state: String,
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pull {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PullState,
    pub is_draft: bool,
    pub author: Option<User>,
    pub head_ref: String,
    pub base_ref: String,
    pub head_oid: String,
    pub created_at: String,
    pub updated_at: String,
    pub merged_at: Option<String>,
    pub closed_at: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
    /// approved | changes_requested | review_required
    pub review: Option<String>,
    pub checks: Option<Checks>,
    /// Numbers of the tasks (issues) this pull request closes.
    pub tasks: Vec<u64>,
    pub comments: u32,
    pub reviewers: Vec<User>,
    pub merge_commit: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub login: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub avatar_url: String,
    pub url: String,
    /// admin | maintain | write | triage | read | member (role unknown)
    pub role: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub oid: String,
    pub headline: String,
    pub committed_at: String,
    pub author_name: String,
    pub author_login: Option<String>,
    pub author_avatar: Option<String>,
    pub url: String,
}

/// One commit on the default branch, reduced to what the activity chart needs.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CommitStamp {
    pub committed_at: String,
    pub author_login: Option<String>,
    pub author_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub id: String,
    pub name_with_owner: String,
    pub name: String,
    pub owner_login: String,
    pub owner_avatar: String,
    /// GitHub's "About" text.
    pub description: Option<String>,
    pub url: String,
    pub homepage_url: Option<String>,
    pub is_private: bool,
    pub is_archived: bool,
    pub has_issues: bool,
    pub created_at: String,
    pub pushed_at: Option<String>,
    pub default_branch: Option<String>,
    pub last_commit: Option<CommitInfo>,
    /// Total commits on the default branch.
    pub total_commits: u64,
    pub branch_count: u32,
    pub open_issues: u32,
    pub closed_issues: u32,
    pub open_pulls: u32,
    pub merged_pulls: u32,
    pub closed_pulls: u32,
    pub stars: u32,
    pub forks: u32,
    pub primary_language: Option<Language>,
    /// ADMIN | MAINTAIN | WRITE | TRIAGE | READ
    pub viewer_permission: Option<String>,
    /// Commits on the default branch in the last `ACTIVITY_DAYS` days.
    pub recent_commits: Vec<CommitStamp>,
    /// True when there were more recent commits than we fetched.
    pub recent_commits_truncated: bool,
}

/// Everything the main screens need for one project, fetched together.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub project_id: String,
    pub fetched_at: String,
    pub viewer: User,
    pub repo: RepoStatus,
    pub tasks: Vec<Task>,
    pub pulls: Vec<Pull>,
    pub members: Vec<Member>,
    pub labels: Vec<Label>,
    /// True when the repo has more issues than this snapshot holds.
    pub tasks_truncated: bool,
    /// True when this came from the on-disk cache, not from GitHub just now.
    #[serde(default)]
    pub from_cache: bool,
}
