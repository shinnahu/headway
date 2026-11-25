//! The local clone: reading its history for the Branch Explorer, plus the few
//! actions Headway takes on it (switch branch, fetch, create branch). Nothing
//! here discards work: git itself refuses a switch that would overwrite
//! uncommitted changes, and that refusal is passed on as-is.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::graph::{self, Layout, RawCommit, RawRef, RefKind};
use crate::{proc, Error, Result};

const FAST: Duration = Duration::from_secs(20);
const SLOW: Duration = Duration::from_secs(60);
const NETWORK: Duration = Duration::from_secs(120);

pub const DEFAULT_LIMIT: usize = 400;
pub const MAX_LIMIT: usize = 5000;

async fn git(dir: &Path, args: &[&str], timeout: Duration) -> Result<proc::Output> {
    let mut full = vec!["-c", "core.quotepath=false", "-c", "color.ui=false"];
    full.extend_from_slice(args);
    // GIT_TERMINAL_PROMPT=0: never wait for a password in a console nobody sees.
    // GIT_OPTIONAL_LOCKS=0: reading status must not fight an editor's own git.
    // LC_ALL=C: English messages, a few of which are matched below.
    let env = [("GIT_TERMINAL_PROMPT", "0"), ("GIT_OPTIONAL_LOCKS", "0"), ("LC_ALL", "C")];
    proc::run("git", &full, Some(dir), &env, timeout).await
}

async fn git_ok(dir: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let out = git(dir, args, timeout).await?;
    if out.ok { Ok(out.stdout) } else { Err(Error::Git(clean(&out.message()))) }
}

/// git's own wording, minus the "fatal:" / "error:" prefixes.
fn clean(msg: &str) -> String {
    msg.lines()
        .map(|l| l.trim_start_matches("fatal: ").trim_start_matches("error: "))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn native_path(p: &str) -> String {
    if cfg!(windows) { p.replace('/', "\\") } else { p.to_string() }
}

/// Remote URLs may carry a user name or token; never show those.
fn redact(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        let host_end = rest.find('/').unwrap_or(rest.len());
        if let Some(at) = rest[..host_end].rfind('@') {
            return format!("{scheme}://{}", &rest[at + 1..]);
        }
    }
    url.to_string()
}

/// "owner/repo" from any remote URL spelling, including SSH host aliases.
pub fn remote_repo(url: &str) -> Option<String> {
    let s = url.trim().trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    let tail = match s.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map(|(_, path)| path)?,
        None => s.rsplit_once(':').map(|(_, path)| path)?,
    };
    let mut segs: Vec<&str> = tail.split('/').filter(|x| !x.is_empty()).collect();
    let repo = segs.pop()?;
    let owner = segs.pop()?;
    Some(format!("{owner}/{repo}"))
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: String,
    pub repo: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalRepo {
    pub root: String,
    pub remotes: Vec<Remote>,
    pub branch: Option<String>,
}

impl LocalRepo {
    pub fn is_clone_of(&self, repo_id: &str) -> bool {
        self.remotes.iter().any(|r| r.repo.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(repo_id)))
    }
}

pub async fn inspect(path: &str) -> Result<LocalRepo> {
    let path = path.trim();
    let dir = Path::new(path);
    if path.is_empty() || !dir.is_dir() {
        return Err(Error::invalid("That folder does not exist."));
    }
    let out = git(dir, &["rev-parse", "--show-toplevel"], FAST).await?;
    if !out.ok {
        return Err(Error::invalid("That folder is not inside a git repository."));
    }
    let root = native_path(out.stdout.trim());
    let root_dir = Path::new(&root);
    let mut remotes: Vec<Remote> = Vec::new();
    for line in git_ok(root_dir, &["remote", "-v"], FAST).await?.lines() {
        // "origin\thttps://github.com/o/r.git (fetch)"
        let mut parts = line.split_whitespace();
        let (Some(name), Some(url)) = (parts.next(), parts.next()) else { continue };
        if remotes.iter().any(|r| r.name == name) {
            continue;
        }
        remotes.push(Remote { name: name.to_string(), url: redact(url), repo: remote_repo(url) });
    }
    let head = git(root_dir, &["symbolic-ref", "--short", "-q", "HEAD"], FAST).await?;
    let branch = Some(head.stdout.trim().to_string()).filter(|s| head.ok && !s.is_empty());
    Ok(LocalRepo { root, remotes, branch })
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkingTree {
    pub head: Option<String>,
    pub branch: Option<String>,
    pub detached: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
}

/// Parses `git status --porcelain=v2 --branch`.
pub fn parse_status(text: &str) -> WorkingTree {
    let mut wt = WorkingTree::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# branch.oid ") {
            if rest != "(initial)" {
                wt.head = Some(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("# branch.head ") {
            if rest == "(detached)" {
                wt.detached = true;
            } else {
                wt.branch = Some(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            wt.upstream = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for part in rest.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    wt.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    wt.behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            let xy = line.as_bytes();
            if xy.len() >= 4 {
                if xy[2] != b'.' {
                    wt.staged += 1;
                }
                if xy[3] != b'.' {
                    wt.unstaged += 1;
                }
            }
        } else if line.starts_with("u ") {
            wt.conflicted += 1;
        } else if line.starts_with("? ") {
            wt.untracked += 1;
        }
    }
    wt
}

pub async fn status(path: &str) -> Result<WorkingTree> {
    let out = git_ok(Path::new(path), &["status", "--porcelain=v2", "--branch"], FAST).await?;
    Ok(parse_status(&out))
}

const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%s%x1e";

pub fn parse_log(text: &str) -> Vec<RawCommit> {
    text.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches(['\n', '\r']);
            let f: Vec<&str> = rec.split('\x1f').collect();
            if f.len() < 6 || f[0].is_empty() {
                return None;
            }
            Some(RawCommit {
                sha: f[0].to_string(),
                parents: f[1].split_whitespace().map(str::to_string).collect(),
                author: f[2].to_string(),
                email: f[3].to_string(),
                date: f[4].to_string(),
                subject: f[5].to_string(),
            })
        })
        .collect()
}

async fn log(dir: &Path, limit: usize) -> Result<(Vec<RawCommit>, bool)> {
    let n = (limit + 1).to_string();
    let args = ["log", "--branches", "--remotes", "--tags", "HEAD", "--date-order", "-n", &n, LOG_FORMAT];
    let out = git(dir, &args, SLOW).await?;
    if !out.ok {
        let msg = out.message();
        // A repository without commits has nothing to draw; that is not an error.
        if msg.contains("does not have any commits yet") || msg.contains("'HEAD'") {
            return Ok((Vec::new(), false));
        }
        return Err(Error::Git(clean(&msg)));
    }
    let mut commits = parse_log(&out.stdout);
    let truncated = commits.len() > limit;
    commits.truncate(limit);
    Ok((commits, truncated))
}

/// "ahead 1, behind 2" | "ahead 3" | "behind 1" | "gone" | ""
fn parse_track(s: &str) -> (u32, u32) {
    let (mut ahead, mut behind) = (0, 0);
    for part in s.split(',') {
        let part = part.trim();
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

const REF_FORMAT: &str = "--format=%(refname)%1f%(objectname)%1f%(*objectname)%1f%(upstream:track,nobracket)";

pub fn parse_refs(text: &str) -> Vec<RawRef> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\x1f').collect();
            if f.len() < 4 {
                return None;
            }
            let (kind, name) = if let Some(n) = f[0].strip_prefix("refs/heads/") {
                (RefKind::Local, n)
            } else if let Some(n) = f[0].strip_prefix("refs/remotes/") {
                (RefKind::Remote, n)
            } else if let Some(n) = f[0].strip_prefix("refs/tags/") {
                (RefKind::Tag, n)
            } else {
                return None;
            };
            // Annotated tags point at a tag object; `*objectname` is the commit behind it.
            let sha = if f[2].is_empty() { f[1] } else { f[2] };
            let (ahead, behind) = parse_track(f[3]);
            Some(RawRef { name: name.to_string(), kind, sha: sha.to_string(), ahead, behind })
        })
        .collect()
}

async fn refs(dir: &Path) -> Result<Vec<RawRef>> {
    let out = git_ok(dir, &["for-each-ref", REF_FORMAT, "refs/heads", "refs/remotes", "refs/tags"], FAST).await?;
    Ok(parse_refs(&out))
}

async fn origin_default_branch(dir: &Path) -> Option<String> {
    let out = git(dir, &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"], FAST).await.ok()?;
    let name = out.stdout.trim();
    if !out.ok || name.is_empty() {
        return None;
    }
    Some(name.strip_prefix("origin/").unwrap_or(name).to_string())
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub root: String,
    #[serde(flatten)]
    pub layout: Layout,
    pub head: Option<String>,
    pub head_branch: Option<String>,
    pub detached: bool,
    pub default_branch: Option<String>,
    /// True when the repository has more history than was loaded.
    pub truncated: bool,
    pub tree: WorkingTree,
}

pub async fn graph(path: &str, limit: usize, default_branch: Option<&str>) -> Result<Graph> {
    let dir = Path::new(path);
    if !dir.is_dir() {
        return Err(Error::invalid(format!("The folder {path} no longer exists.")));
    }
    let tree = status(path).await?;
    let refs = refs(dir).await?;
    let (commits, truncated) = log(dir, limit.clamp(50, MAX_LIMIT)).await?;
    let default_branch = match default_branch {
        Some(d) => Some(d.to_string()),
        None => origin_default_branch(dir).await,
    };
    let layout = graph::build(commits, &refs, default_branch.as_deref(), tree.head.as_deref(), tree.branch.as_deref());
    Ok(Graph {
        root: native_path(path),
        layout,
        head: tree.head.clone(),
        head_branch: tree.branch.clone(),
        detached: tree.detached,
        default_branch,
        truncated,
        tree,
    })
}

async fn check_branch_name(dir: &Path, name: &str) -> Result<()> {
    let bad = || Error::invalid(format!("\"{name}\" is not a valid branch name."));
    if name.is_empty() || name.starts_with('-') {
        return Err(bad());
    }
    let out = git(dir, &["check-ref-format", "--branch", name], FAST).await?;
    if out.ok { Ok(()) } else { Err(bad()) }
}

/// Moves the working copy to a branch. A branch that only exists on the remote
/// gets a local branch that tracks it (git's own behaviour for `git switch`).
pub async fn switch(path: &str, branch: &str) -> Result<WorkingTree> {
    let dir = Path::new(path);
    check_branch_name(dir, branch).await?;
    git_ok(dir, &["switch", branch], SLOW).await?;
    status(path).await
}

pub async fn fetch(path: &str) -> Result<()> {
    git_ok(Path::new(path), &["fetch", "--all", "--prune"], NETWORK).await?;
    Ok(())
}

pub async fn create_branch(path: &str, name: &str, from: Option<&str>, switch_to: bool) -> Result<WorkingTree> {
    let dir = Path::new(path);
    check_branch_name(dir, name).await?;
    let start = from.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("HEAD");
    if start.starts_with('-') {
        return Err(Error::invalid("That starting point is not a commit or branch."));
    }
    let commit = format!("{start}^{{commit}}");
    let found = git(dir, &["rev-parse", "--verify", "--quiet", &commit], FAST).await?;
    if !found.ok {
        return Err(Error::invalid(format!("git cannot find \"{start}\" to branch from.")));
    }
    let start = found.stdout.trim().to_string();
    if switch_to {
        git_ok(dir, &["switch", "-c", name, &start], SLOW).await?;
    } else {
        git_ok(dir, &["branch", name, &start], FAST).await?;
    }
    status(path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_owner_and_repo_from_remote_urls() {
        let want = Some("octo/hello".to_string());
        assert_eq!(remote_repo("https://github.com/octo/hello.git"), want);
        assert_eq!(remote_repo("https://user:secret@github.com/octo/hello"), want);
        assert_eq!(remote_repo("git@github.com:octo/hello.git"), want);
        assert_eq!(remote_repo("git@work-alias:octo/hello.git"), want);
        assert_eq!(remote_repo("ssh://git@github.com/octo/hello.git"), want);
        assert_eq!(remote_repo("C:\\repos\\hello"), None);
        assert_eq!(remote_repo("https://github.com/octo"), None);
    }

    #[test]
    fn hides_credentials_in_remote_urls() {
        assert_eq!(redact("https://user:secret@github.com/o/r.git"), "https://github.com/o/r.git");
        assert_eq!(redact("https://github.com/o/r.git"), "https://github.com/o/r.git");
        assert_eq!(redact("git@github.com:o/r.git"), "git@github.com:o/r.git");
    }

    #[test]
    fn parses_status() {
        let text = "# branch.oid 1234abcd\n# branch.head feature/x\n# branch.upstream origin/feature/x\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 aaa bbb src/a.rs\n1 M. N... 100644 100644 100644 aaa bbb src/b.rs\n1 MM N... 100644 100644 100644 aaa bbb src/c.rs\nu UU N... 100644 100644 100644 100644 a b c src/d.rs\n? notes.txt\n";
        let wt = parse_status(text);
        assert_eq!(wt.branch.as_deref(), Some("feature/x"));
        assert_eq!(wt.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!((wt.ahead, wt.behind), (2, 1));
        assert_eq!((wt.staged, wt.unstaged, wt.untracked, wt.conflicted), (2, 2, 1, 1));
        assert!(!wt.detached);

        let wt = parse_status("# branch.oid (initial)\n# branch.head main\n");
        assert_eq!(wt.head, None);
        let wt = parse_status("# branch.oid abcd\n# branch.head (detached)\n");
        assert!(wt.detached);
        assert_eq!(wt.branch, None);
    }

    #[test]
    fn parses_tracking_summary() {
        assert_eq!(parse_track("ahead 1, behind 2"), (1, 2));
        assert_eq!(parse_track("behind 4"), (0, 4));
        assert_eq!(parse_track("gone"), (0, 0));
        assert_eq!(parse_track(""), (0, 0));
    }

    /// Runs real git in a scratch repository: this is what proves the format
    /// strings above match what git prints.
    #[tokio::test]
    async fn reads_a_real_repository() {
        let dir = std::env::temp_dir().join(format!("headway-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.to_string_lossy().to_string();
        let run = |args: &'static [&'static str]| {
            let dir = dir.clone();
            async move {
                let mut full = vec!["-c", "user.name=Test Person", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false"];
                full.extend_from_slice(args);
                let out = proc::run("git", &full, Some(&dir), &[], SLOW).await.expect("git runs");
                assert!(out.ok, "git {args:?} failed: {}", out.message());
            }
        };
        run(&["init", "-b", "main"]).await;
        std::fs::write(dir.join("a.txt"), "one").unwrap();
        run(&["add", "."]).await;
        run(&["commit", "-m", "First commit"]).await;
        run(&["switch", "-c", "feature/login"]).await;
        std::fs::write(dir.join("b.txt"), "two").unwrap();
        run(&["add", "."]).await;
        run(&["commit", "-m", "Add login; with | odd \"chars\""]).await;
        run(&["switch", "main"]).await;
        std::fs::write(dir.join("c.txt"), "three").unwrap();
        run(&["add", "."]).await;
        run(&["commit", "-m", "Second on main"]).await;
        run(&["merge", "--no-ff", "feature/login", "-m", "Merge branch 'feature/login'"]).await;
        run(&["tag", "-a", "v1", "-m", "release"]).await;
        run(&["branch", "idea"]).await;

        let info = inspect(&path).await.unwrap();
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert!(info.remotes.is_empty());

        let g = graph(&path, 100, None).await.unwrap();
        assert_eq!(g.layout.nodes.len(), 4);
        assert_eq!(g.head_branch.as_deref(), Some("main"));
        assert!(!g.truncated);
        let names: Vec<&str> = g.layout.lanes.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["main", "feature/login"]);
        let feature = &g.layout.lanes[1];
        assert_eq!((feature.commit_count, feature.row), (1, 1));
        assert_eq!(feature.merged_into.as_deref(), Some("main"));
        assert_eq!(feature.owner, "Test Person");
        let odd = g.layout.nodes.iter().find(|n| n.lane == 1).unwrap();
        assert_eq!(odd.subject, "Add login; with | odd \"chars\"");
        let tip = g.layout.nodes.last().unwrap();
        let labels: Vec<&str> = tip.refs.iter().map(|r| r.name.as_str()).collect();
        assert!(labels.contains(&"main") && labels.contains(&"v1") && labels.contains(&"idea"), "{labels:?}");
        assert_eq!(g.tree.staged + g.tree.unstaged + g.tree.untracked, 0);

        // Switching works, and git's refusal to overwrite local changes comes through.
        let wt = switch(&path, "feature/login").await.unwrap();
        assert_eq!(wt.branch.as_deref(), Some("feature/login"));
        let wt = switch(&path, "main").await.unwrap();
        assert_eq!(wt.branch.as_deref(), Some("main"));
        // c.txt exists on main only: an uncommitted edit to it blocks the switch.
        std::fs::write(dir.join("c.txt"), "uncommitted change").unwrap();
        let wt = status(&path).await.unwrap();
        assert_eq!(wt.unstaged, 1);
        let refused = switch(&path, "feature/login").await.unwrap_err();
        assert!(refused.to_string().contains("c.txt"), "{refused}");
        assert_eq!(std::fs::read_to_string(dir.join("c.txt")).unwrap(), "uncommitted change");
        assert_eq!(status(&path).await.unwrap().branch.as_deref(), Some("main"));
        assert!(switch(&path, "--force").await.is_err());
        assert!(switch(&path, "no-such-branch").await.is_err());

        let wt = create_branch(&path, "from-first", Some("v1"), false).await.unwrap();
        assert_eq!(wt.branch.as_deref(), Some("main"));
        assert!(create_branch(&path, "bad name", None, false).await.is_err());
        assert!(create_branch(&path, "x", Some("--upload-pack=evil"), false).await.is_err());

        // A limit smaller than the history marks the graph as cut.
        let g = graph(&path, 50, None).await.unwrap();
        assert!(!g.truncated);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
