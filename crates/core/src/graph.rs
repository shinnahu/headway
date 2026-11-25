//! Turns git history into the Branch Explorer picture: every branch is a
//! horizontal lane, time runs left to right, merges are links between lanes.
//!
//! Git does not record which branch a commit was made on, so lanes are
//! reconstructed: a branch owns the commits on its first-parent chain down to
//! the point where another branch already owns them. Branches that were merged
//! and deleted get their name back from the merge commit's message.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct RawCommit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub date: String,
    pub subject: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Clone, Debug)]
pub struct RawRef {
    /// Short name: "main", "origin/main", "v1.0".
    pub name: String,
    pub kind: RefKind,
    pub sha: String,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub sha: String,
    pub author: String,
    pub email: String,
    pub date: String,
    pub subject: String,
    pub lane: usize,
    /// Position on the time axis; also this node's index in `nodes`.
    pub col: usize,
    pub refs: Vec<RefLabel>,
    pub parent_count: usize,
    /// True when a parent is older than the loaded history.
    pub hidden_parents: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    pub id: usize,
    pub name: String,
    pub row: usize,
    pub first_col: usize,
    pub last_col: usize,
    /// Column where the lane's picture starts/ends once links are counted.
    pub from_col: usize,
    pub to_col: usize,
    pub tip: String,
    /// The commit this branch started from (in another lane).
    pub base: Option<String>,
    /// both | local | remote | merged | loose
    pub kind: String,
    pub local: bool,
    pub remote: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub is_default: bool,
    pub is_current: bool,
    /// Date and author of the branch's first own commit.
    pub created_at: String,
    pub owner: String,
    pub commit_count: usize,
    pub merged_into: Option<String>,
    /// Pull request number, when the merge commit names one.
    pub pull: Option<u64>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// Next commit on the same branch.
    Flow,
    /// A branch starting from another one.
    Fork,
    /// A branch merged into another one.
    Merge,
}

#[derive(Serialize, Clone, Debug)]
pub struct Link {
    /// Column of the parent commit.
    pub from: usize,
    /// Column of the child commit.
    pub to: usize,
    pub kind: LinkKind,
}

/// A branch name people can switch to, whether or not it owns a lane.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub local: bool,
    pub remote: Option<String>,
    pub tip: String,
    pub lane: Option<usize>,
    pub ahead: u32,
    pub behind: u32,
    pub is_default: bool,
    pub is_current: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Layout {
    pub nodes: Vec<Node>,
    pub lanes: Vec<Lane>,
    pub links: Vec<Link>,
    pub branches: Vec<Branch>,
}

static RE_PR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Merge pull request #(\d+) from [^/\s]+/(\S+)").expect("valid regex"));
static RE_BRANCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Merge (remote-tracking )?branch '([^']+)'").expect("valid regex"));
static RE_BITBUCKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Merged in (\S+) \(pull request #(\d+)\)").expect("valid regex"));

/// The branch a merge commit says it merged, and the pull request if named.
pub fn merged_branch(subject: &str) -> Option<(String, Option<u64>)> {
    if let Some(c) = RE_PR.captures(subject) {
        return Some((c[2].to_string(), c[1].parse().ok()));
    }
    if let Some(c) = RE_BRANCH.captures(subject) {
        let mut name = c[2].to_string();
        if c.get(1).is_some() {
            if let Some((_, rest)) = name.split_once('/') {
                name = rest.to_string();
            }
        }
        return Some((name, None));
    }
    if let Some(c) = RE_BITBUCKET.captures(subject) {
        return Some((c[1].to_string(), c[2].parse().ok()));
    }
    None
}

struct Tip {
    ref_name: String,
    kind: RefKind,
    at: usize,
    ahead: u32,
    behind: u32,
}

struct Logical {
    name: String,
    tips: Vec<Tip>,
}

#[derive(Default)]
struct LaneBuild {
    name: String,
    logical: Option<usize>,
    /// Oldest first.
    commits: Vec<usize>,
    base: Option<usize>,
    local: bool,
    remote: Option<String>,
    ahead: u32,
    behind: u32,
    merged_into: Option<usize>,
    pull: Option<u64>,
    loose: bool,
}

struct Builder {
    parents: Vec<Vec<Option<usize>>>,
    lane_of: Vec<Option<usize>>,
    lanes: Vec<LaneBuild>,
}

impl Builder {
    fn first_parent(&self, i: usize) -> Option<usize> {
        self.parents[i].first().copied().flatten()
    }

    /// The unowned commits on the first-parent chain from `start` (newest
    /// first), and the owned commit where the chain stopped.
    fn claim(&self, start: usize) -> (Vec<usize>, Option<usize>) {
        let mut chain = Vec::new();
        let mut cur = Some(start);
        while let Some(i) = cur {
            if self.lane_of[i].is_some() {
                return (chain, Some(i));
            }
            chain.push(i);
            cur = self.first_parent(i);
        }
        (chain, None)
    }

    fn new_lane(&mut self, mut lane: LaneBuild, chain: &[usize]) -> usize {
        let id = self.lanes.len();
        lane.commits = chain.iter().rev().copied().collect();
        for &i in chain {
            self.lane_of[i] = Some(id);
        }
        self.lanes.push(lane);
        id
    }

    fn place_logical(&mut self, li: usize, logical: &Logical) {
        let mut tips: Vec<&Tip> = logical.tips.iter().collect();
        tips.sort_by_key(|t| t.at);
        let mut own: Vec<usize> = Vec::new();
        for tip in tips {
            let (chain, stop) = self.claim(tip.at);
            let lane = if chain.is_empty() {
                // The tip already sits in a lane. If that lane is ours the ref
                // is simply behind; if not, the branch has no commits of its own.
                match self.lane_of[tip.at] {
                    Some(l) if own.contains(&l) => l,
                    _ => continue,
                }
            } else {
                let continues = stop
                    .and_then(|s| self.lane_of[s].map(|l| (s, l)))
                    .filter(|(s, l)| own.contains(l) && self.lanes[*l].commits.last() == Some(s));
                match continues {
                    Some((_, l)) => {
                        for &i in chain.iter().rev() {
                            self.lane_of[i] = Some(l);
                            self.lanes[l].commits.push(i);
                        }
                        l
                    }
                    None => {
                        let l = self.new_lane(
                            LaneBuild { name: logical.name.clone(), logical: Some(li), base: stop, ..Default::default() },
                            &chain,
                        );
                        own.push(l);
                        l
                    }
                }
            };
            match tip.kind {
                RefKind::Local => {
                    self.lanes[lane].local = true;
                    self.lanes[lane].ahead = tip.ahead;
                    self.lanes[lane].behind = tip.behind;
                }
                RefKind::Remote => self.lanes[lane].remote = Some(tip.ref_name.clone()),
                RefKind::Tag => {}
            }
        }
        // Local and remote went different ways: two lanes, so name the remote one fully.
        if own.len() > 1 {
            for l in own {
                if !self.lanes[l].local {
                    if let Some(r) = self.lanes[l].remote.clone() {
                        self.lanes[l].name = r;
                    }
                }
            }
        }
    }
}

/// `commits` come newest first, as `git log --date-order` prints them.
pub fn build(
    mut commits: Vec<RawCommit>,
    refs: &[RawRef],
    default_branch: Option<&str>,
    head: Option<&str>,
    head_branch: Option<&str>,
) -> Layout {
    commits.reverse(); // oldest first: a parent is always left of its children
    let n = commits.len();
    if n == 0 {
        return Layout::default();
    }
    let index: HashMap<&str, usize> = commits.iter().enumerate().map(|(i, c)| (c.sha.as_str(), i)).collect();
    let parents: Vec<Vec<Option<usize>>> =
        commits.iter().map(|c| c.parents.iter().map(|p| index.get(p.as_str()).copied()).collect()).collect();

    // Group refs into logical branches: "main" and "origin/main" are one branch.
    let mut logicals: Vec<Logical> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for r in refs {
        if r.kind == RefKind::Tag || r.name.ends_with("/HEAD") || r.name == "HEAD" {
            continue;
        }
        let Some(&at) = index.get(r.sha.as_str()) else { continue };
        let name = match r.kind {
            RefKind::Remote => r.name.split_once('/').map(|(_, rest)| rest).unwrap_or(&r.name).to_string(),
            _ => r.name.clone(),
        };
        let li = *by_name.entry(name.clone()).or_insert_with(|| {
            logicals.push(Logical { name, tips: Vec::new() });
            logicals.len() - 1
        });
        logicals[li].tips.push(Tip { ref_name: r.name.clone(), kind: r.kind, at, ahead: r.ahead, behind: r.behind });
    }

    let default_li = default_branch
        .and_then(|d| by_name.get(d).copied())
        .or_else(|| ["main", "master", "trunk", "develop"].iter().find_map(|d| by_name.get(*d).copied()))
        .or_else(|| head_branch.and_then(|h| by_name.get(h).copied()))
        .or(if logicals.is_empty() { None } else { Some(0) });

    let mut b = Builder { parents, lane_of: vec![None; n], lanes: Vec::new() };
    if let Some(li) = default_li {
        b.place_logical(li, &logicals[li]);
    }
    let default_lane = if b.lanes.is_empty() { None } else { Some(0) };

    // Order the other branches. A branch whose tip lies on another branch's
    // chain (stacked work) goes first; among unrelated ones the longer goes first.
    let chains: Vec<Vec<usize>> = logicals
        .iter()
        .map(|l| l.tips.iter().map(|t| t.at).max().map(|start| b.claim(start).0).unwrap_or_default())
        .collect();
    let mut on_chain: HashMap<usize, usize> = HashMap::new();
    for chain in &chains {
        for &c in chain.iter().skip(1) {
            *on_chain.entry(c).or_insert(0) += 1;
        }
    }
    let mut order: Vec<usize> = (0..logicals.len()).filter(|li| Some(*li) != default_li).collect();
    order.sort_by(|&x, &y| {
        let inside = |li: usize| chains[li].first().and_then(|t| on_chain.get(t)).copied().unwrap_or(0);
        inside(y)
            .cmp(&inside(x))
            .then_with(|| chains[y].len().cmp(&chains[x].len()))
            .then_with(|| logicals[x].name.cmp(&logicals[y].name))
    });
    for li in order {
        b.place_logical(li, &logicals[li]);
    }

    // What is left was merged from branches that no longer exist (or is only
    // reachable from a tag / a detached HEAD).
    let head_at = head.and_then(|h| index.get(h).copied());
    for c in (0..n).rev() {
        if b.lane_of[c].is_none() {
            let (chain, stop) = b.claim(c);
            let name = if Some(c) == head_at { "HEAD (detached)" } else { "no branch" };
            b.new_lane(LaneBuild { name: name.to_string(), base: stop, loose: true, ..Default::default() }, &chain);
        }
        for pi in 1..b.parents[c].len() {
            let Some(p) = b.parents[c][pi] else { continue };
            if b.lane_of[p].is_some() {
                continue;
            }
            let (chain, stop) = b.claim(p);
            let (name, pull) = match merged_branch(&commits[c].subject) {
                Some((name, pull)) if pi == 1 => (name, pull),
                _ => ("merged branch".to_string(), None),
            };
            let into = b.lane_of[c];
            b.new_lane(LaneBuild { name, base: stop, merged_into: into, pull, ..Default::default() }, &chain);
        }
    }

    // Links, and how far each lane's picture reaches because of them.
    let lane_of: Vec<usize> = b.lane_of.iter().map(|l| l.expect("every commit has a lane")).collect();
    let mut links: Vec<Link> = Vec::new();
    let mut from_col: Vec<usize> = b.lanes.iter().map(|l| l.commits[0]).collect();
    let mut to_col: Vec<usize> = b.lanes.iter().map(|l| *l.commits.last().expect("lanes are never empty")).collect();
    for c in 0..n {
        for (pi, p) in b.parents[c].iter().enumerate() {
            let Some(p) = *p else { continue };
            let (lc, lp) = (lane_of[c], lane_of[p]);
            let kind = if pi > 0 {
                LinkKind::Merge
            } else if lc == lp {
                LinkKind::Flow
            } else {
                LinkKind::Fork
            };
            if lc != lp {
                from_col[lc] = from_col[lc].min(p);
                to_col[lp] = to_col[lp].max(c);
                if kind == LinkKind::Merge && b.lanes[lp].commits.last() == Some(&p) {
                    b.lanes[lp].merged_into = Some(lc);
                }
            }
            links.push(Link { from: p, to: c, kind });
        }
    }

    // Rows: the default branch on top, the others packed below without overlap.
    let mut row: Vec<usize> = vec![0; b.lanes.len()];
    let mut order: Vec<usize> = (0..b.lanes.len()).filter(|l| Some(*l) != default_lane).collect();
    order.sort_by_key(|&l| (from_col[l], to_col[l]));
    // Per row: the last column used, and whether that end is a link (the branch
    // was merged there) rather than a commit. A branch that was merged into a
    // commit and one that starts from that same commit may share a row.
    let mut rows: Vec<(usize, bool)> = Vec::new();
    let first_free_row = if default_lane.is_some() { 1 } else { 0 };
    for l in order {
        let first = b.lanes[l].commits[0];
        let last = *b.lanes[l].commits.last().expect("lanes are never empty");
        let starts_with_link = from_col[l] < first;
        let slot = rows.iter().position(|&(end, link_end)| {
            end < from_col[l] || (end == from_col[l] && link_end && starts_with_link)
        });
        let entry = (to_col[l], to_col[l] > last);
        match slot {
            Some(r) => {
                rows[r] = entry;
                row[l] = r + first_free_row;
            }
            None => {
                rows.push(entry);
                row[l] = rows.len() - 1 + first_free_row;
            }
        }
    }

    // Ref labels on commits.
    let mut labels: Vec<Vec<RefLabel>> = vec![Vec::new(); n];
    for r in refs {
        if r.name.ends_with("/HEAD") || r.name == "HEAD" {
            continue;
        }
        if let Some(&at) = index.get(r.sha.as_str()) {
            labels[at].push(RefLabel { name: r.name.clone(), kind: r.kind });
        }
    }

    let lanes: Vec<Lane> = b
        .lanes
        .iter()
        .enumerate()
        .map(|(id, l)| {
            let first = l.commits[0];
            let last = *l.commits.last().expect("lanes are never empty");
            let kind = if l.logical.is_some() {
                match (l.local, l.remote.is_some()) {
                    (true, true) => "both",
                    (true, false) => "local",
                    _ => "remote",
                }
            } else if l.loose {
                "loose"
            } else {
                "merged"
            };
            let logical_name = l.logical.map(|li| logicals[li].name.as_str());
            Lane {
                id,
                name: l.name.clone(),
                row: row[id],
                first_col: first,
                last_col: last,
                from_col: from_col[id],
                to_col: to_col[id],
                tip: commits[last].sha.clone(),
                base: l.base.map(|i| commits[i].sha.clone()),
                kind: kind.to_string(),
                local: l.local,
                remote: l.remote.clone(),
                ahead: l.ahead,
                behind: l.behind,
                is_default: Some(id) == default_lane,
                is_current: l.local && head_branch.is_some() && logical_name == head_branch,
                created_at: commits[first].date.clone(),
                owner: commits[first].author.clone(),
                commit_count: l.commits.len(),
                merged_into: l.merged_into.map(|t| b.lanes[t].name.clone()),
                pull: l.pull,
            }
        })
        .collect();

    let branches: Vec<Branch> = logicals
        .iter()
        .enumerate()
        .map(|(li, lg)| {
            let local = lg.tips.iter().find(|t| t.kind == RefKind::Local);
            let remote = lg.tips.iter().find(|t| t.kind == RefKind::Remote);
            let tip = local.or(remote).map(|t| t.at).unwrap_or(0);
            let lane = b.lanes.iter().position(|l| l.logical == Some(li) && (l.local || local.is_none()));
            Branch {
                name: lg.name.clone(),
                local: local.is_some(),
                remote: remote.map(|t| t.ref_name.clone()),
                tip: commits[tip].sha.clone(),
                lane,
                ahead: local.map(|t| t.ahead).unwrap_or(0),
                behind: local.map(|t| t.behind).unwrap_or(0),
                is_default: Some(li) == default_li,
                is_current: local.is_some() && head_branch == Some(lg.name.as_str()),
            }
        })
        .collect();

    let nodes: Vec<Node> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| Node {
            sha: c.sha.clone(),
            author: c.author.clone(),
            email: c.email.clone(),
            date: c.date.clone(),
            subject: c.subject.clone(),
            lane: lane_of[i],
            col: i,
            refs: std::mem::take(&mut labels[i]),
            parent_count: c.parents.len(),
            hidden_parents: b.parents[i].iter().any(Option::is_none),
        })
        .collect();

    Layout { nodes, lanes, links, branches }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Commits are given oldest first for readability and reversed for `build`.
    fn history(spec: &[(&str, &[&str], &str)]) -> Vec<RawCommit> {
        spec.iter()
            .rev()
            .map(|(sha, parents, subject)| RawCommit {
                sha: sha.to_string(),
                parents: parents.iter().map(|p| p.to_string()).collect(),
                author: "Ana".into(),
                email: "ana@example.com".into(),
                date: "2026-11-20T10:00:00+00:00".into(),
                subject: subject.to_string(),
            })
            .collect()
    }

    fn local(name: &str, sha: &str) -> RawRef {
        RawRef { name: name.into(), kind: RefKind::Local, sha: sha.into(), ahead: 0, behind: 0 }
    }

    fn remote(name: &str, sha: &str) -> RawRef {
        RawRef { name: name.into(), kind: RefKind::Remote, sha: sha.into(), ahead: 0, behind: 0 }
    }

    fn lane<'a>(g: &'a Layout, name: &str) -> &'a Lane {
        g.lanes.iter().find(|l| l.name == name).unwrap_or_else(|| panic!("no lane {name}"))
    }

    fn lane_shas(g: &Layout, name: &str) -> Vec<String> {
        let id = lane(g, name).id;
        g.nodes.iter().filter(|n| n.lane == id).map(|n| n.sha.clone()).collect()
    }

    #[test]
    fn linear_history_is_one_lane() {
        let g = build(
            history(&[("a", &[], "one"), ("b", &["a"], "two"), ("c", &["b"], "three")]),
            &[local("main", "c"), remote("origin/main", "c")],
            Some("main"),
            Some("c"),
            Some("main"),
        );
        assert_eq!(g.lanes.len(), 1);
        let main = lane(&g, "main");
        assert_eq!((main.row, main.first_col, main.last_col, main.commit_count), (0, 0, 2, 3));
        assert_eq!(main.kind, "both");
        assert!(main.is_default && main.is_current);
        assert_eq!(g.links.len(), 2);
        assert!(g.links.iter().all(|l| l.kind == LinkKind::Flow));
        assert_eq!(g.nodes[2].refs.len(), 2);
    }

    #[test]
    fn open_feature_branch_gets_its_own_lane() {
        // a - b - d        main
        //      \
        //       c          feature
        let g = build(
            history(&[("a", &[], "1"), ("b", &["a"], "2"), ("c", &["b"], "feature work"), ("d", &["b"], "3")]),
            &[local("main", "d"), local("feature", "c")],
            Some("main"),
            Some("d"),
            Some("main"),
        );
        assert_eq!(lane_shas(&g, "main"), ["a", "b", "d"]);
        assert_eq!(lane_shas(&g, "feature"), ["c"]);
        let f = lane(&g, "feature");
        assert_eq!(f.row, 1);
        assert_eq!(f.base.as_deref(), Some("b"));
        assert_eq!(f.kind, "local");
        assert!(!f.is_current);
        assert_eq!(f.from_col, 1); // the fork link starts at b
        let fork = g.links.iter().find(|l| l.kind == LinkKind::Fork).unwrap();
        assert_eq!((fork.from, fork.to), (1, 2));
    }

    #[test]
    fn merged_and_deleted_branch_is_named_from_the_merge_commit() {
        // a - b - m        main        (m merges c)
        //  \     /
        //    c
        let g = build(
            history(&[
                ("a", &[], "1"),
                ("b", &["a"], "2"),
                ("c", &["a"], "work"),
                ("m", &["b", "c"], "Merge pull request #4 from octo/fix-login"),
            ]),
            &[local("main", "m")],
            Some("main"),
            Some("m"),
            Some("main"),
        );
        assert_eq!(lane_shas(&g, "main"), ["a", "b", "m"]);
        let f = lane(&g, "fix-login");
        assert_eq!(f.kind, "merged");
        assert_eq!(f.pull, Some(4));
        assert_eq!(f.merged_into.as_deref(), Some("main"));
        assert_eq!(f.base.as_deref(), Some("a"));
        assert!(g.links.iter().any(|l| l.kind == LinkKind::Merge && l.to == 3));
        assert!(g.branches.iter().all(|b| b.name != "fix-login"));
    }

    #[test]
    fn local_behind_remote_is_still_one_lane() {
        let g = build(
            history(&[("a", &[], "1"), ("b", &["a"], "2"), ("c", &["b"], "3")]),
            &[local("main", "b"), remote("origin/main", "c")],
            Some("main"),
            Some("b"),
            Some("main"),
        );
        assert_eq!(g.lanes.len(), 1);
        assert_eq!(lane_shas(&g, "main"), ["a", "b", "c"]);
        assert_eq!(lane(&g, "main").kind, "both");
    }

    #[test]
    fn diverged_local_and_remote_become_two_lanes() {
        // a - x      main (local, unpushed)
        //  \
        //    y       origin/main
        let g = build(
            history(&[("a", &[], "1"), ("x", &["a"], "mine"), ("y", &["a"], "theirs")]),
            &[local("main", "x"), remote("origin/main", "y")],
            Some("main"),
            Some("x"),
            Some("main"),
        );
        assert_eq!(g.lanes.len(), 2);
        assert_eq!(lane_shas(&g, "main"), ["a", "x"]);
        assert_eq!(lane_shas(&g, "origin/main"), ["y"]);
        assert_eq!(lane(&g, "origin/main").kind, "remote");
        assert!(lane(&g, "main").is_current);
    }

    #[test]
    fn stacked_branches_keep_their_own_commits() {
        // a - b                main
        //      \
        //       c - d          one
        //            \
        //             e        two (stacked on one)
        let g = build(
            history(&[("a", &[], "1"), ("b", &["a"], "2"), ("c", &["b"], "3"), ("d", &["c"], "4"), ("e", &["d"], "5")]),
            &[local("main", "b"), local("two", "e"), local("one", "d")],
            Some("main"),
            None,
            None,
        );
        assert_eq!(lane_shas(&g, "one"), ["c", "d"]);
        assert_eq!(lane_shas(&g, "two"), ["e"]);
        assert_eq!(lane(&g, "two").base.as_deref(), Some("d"));
    }

    #[test]
    fn long_lived_branch_wins_over_a_short_one_that_forked_from_it() {
        // a                         main
        //  \
        //   r1 - r2 - r3 - r4       release
        //     \
        //      f1                   feature (forked from release early)
        let g = build(
            history(&[
                ("a", &[], "1"),
                ("r1", &["a"], "r"),
                ("f1", &["r1"], "f"),
                ("r2", &["r1"], "r"),
                ("r3", &["r2"], "r"),
                ("r4", &["r3"], "r"),
            ]),
            &[local("main", "a"), local("feature", "f1"), local("release", "r4")],
            Some("main"),
            None,
            None,
        );
        assert_eq!(lane_shas(&g, "release"), ["r1", "r2", "r3", "r4"]);
        assert_eq!(lane_shas(&g, "feature"), ["f1"]);
    }

    #[test]
    fn finished_branches_share_a_row() {
        // main: a b m1 e m2 ; two short branches merged one after the other
        let g = build(
            history(&[
                ("a", &[], "1"),
                ("b", &["a"], "2"),
                ("c", &["a"], "x"),
                ("m1", &["b", "c"], "Merge branch 'first'"),
                ("d", &["m1"], "y"),
                ("e", &["m1"], "3"),
                ("m2", &["e", "d"], "Merge branch 'second' into main"),
            ]),
            &[local("main", "m2")],
            Some("main"),
            None,
            None,
        );
        assert_eq!(lane(&g, "first").row, 1);
        assert_eq!(lane(&g, "second").row, 1);
        assert_eq!(lane(&g, "main").row, 0);
    }

    #[test]
    fn branch_without_own_commits_is_listed_but_has_no_lane() {
        let g = build(
            history(&[("a", &[], "1"), ("b", &["a"], "2")]),
            &[local("main", "b"), local("idea", "b")],
            Some("main"),
            Some("b"),
            Some("idea"),
        );
        assert_eq!(g.lanes.len(), 1);
        let idea = g.branches.iter().find(|b| b.name == "idea").unwrap();
        assert_eq!(idea.lane, None);
        assert!(idea.is_current);
        assert_eq!(g.nodes[1].refs.len(), 2);
    }

    #[test]
    fn history_cut_by_the_limit_is_marked() {
        let g = build(history(&[("b", &["a"], "2"), ("c", &["b"], "3")]), &[local("main", "c")], Some("main"), None, None);
        assert!(g.nodes[0].hidden_parents);
        assert!(!g.nodes[1].hidden_parents);
        assert_eq!(g.links.len(), 1);
    }

    #[test]
    fn reads_merge_messages() {
        assert_eq!(merged_branch("Merge pull request #12 from octo/feature/x"), Some(("feature/x".into(), Some(12))));
        assert_eq!(merged_branch("Merge branch 'fix' into main"), Some(("fix".into(), None)));
        assert_eq!(merged_branch("Merge remote-tracking branch 'origin/dev'"), Some(("dev".into(), None)));
        assert_eq!(merged_branch("Merged in hot/fix (pull request #5)"), Some(("hot/fix".into(), Some(5))));
        assert_eq!(merged_branch("Fix the merge"), None);
    }
}
