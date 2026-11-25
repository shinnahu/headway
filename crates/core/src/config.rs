//! Local settings: who is signed in, which projects exist, where their clones
//! live. Stored as JSON under the user's config folder. No secrets in here:
//! tokens live in the OS credential store (see `auth`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const APP_DIR: &str = "Headway";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthSource {
    /// Use the GitHub CLI's session (`gh auth token`). Nothing is stored by us.
    Gh,
    /// A token kept in the OS credential store (device flow or pasted token).
    Keyring,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub source: AuthSource,
    pub login: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// "owner/repo" exactly as GitHub spells it.
    pub id: String,
    pub owner: String,
    pub repo: String,
    /// Folder of the local clone, if the user pointed us at one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(default)]
    pub added_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// How often the app asks GitHub whether anything changed.
    pub sync_seconds: u32,
    /// Client ID of the team's own GitHub OAuth App (enables "Sign in with GitHub").
    pub github_client_id: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { sync_seconds: 30, github_client_id: None }
    }
}

/// A calendar connected by its private link. Only the name lives here: the
/// link itself is a secret and is kept in the OS credential store.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CalendarRef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub added_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub auth: Option<AuthConfig>,
    pub projects: Vec<Project>,
    pub current_project: Option<String>,
    pub settings: Settings,
    pub calendars: Vec<CalendarRef>,
}

pub fn config_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("HEADWAY_CONFIG_DIR") {
        return PathBuf::from(p);
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join(APP_DIR)
}

pub fn cache_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("HEADWAY_CACHE_DIR") {
        return PathBuf::from(p);
    }
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".")).join(APP_DIR).join("cache")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

impl Config {
    /// A missing file is a fresh install. A file we cannot parse is kept aside
    /// as `config.broken.json` so nothing the user had is silently destroyed.
    pub fn load(path: &Path) -> Config {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return Config::default(),
        };
        match serde_json::from_str::<Config>(&text) {
            Ok(c) => c,
            Err(_) => {
                let _ = std::fs::rename(path, path.with_file_name("config.broken.json"));
                Config::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn project(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id.eq_ignore_ascii_case(id))
    }

    pub fn project_mut(&mut self, id: &str) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.id.eq_ignore_ascii_case(id))
    }

    pub fn current(&self) -> Option<&Project> {
        self.current_project
            .as_deref()
            .and_then(|id| self.project(id))
            .or_else(|| self.projects.first())
    }

    /// The project to use when a caller names none: the one given, else the
    /// current one.
    pub fn resolve(&self, id: Option<&str>) -> Result<&Project> {
        match id {
            Some(id) if !id.trim().is_empty() => self
                .project(id.trim())
                .ok_or_else(|| Error::NotFound(format!("No project named \"{id}\" is set up in Headway."))),
            _ => self
                .current()
                .ok_or_else(|| Error::NotFound("No project is set up in Headway yet.".into())),
        }
    }
}

/// Splits "owner/repo" (also accepts a full GitHub URL or a git remote URL).
pub fn parse_repo(input: &str) -> Option<(String, String)> {
    let s = input.trim().trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    let tail = if let Some(i) = s.find("github.com") {
        s[i + "github.com".len()..].trim_start_matches([':', '/'])
    } else {
        s
    };
    let mut parts = tail.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    let ok = |p: &str| {
        !p.starts_with('-')
            && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if ok(owner) && ok(repo) {
        Some((owner.to_string(), repo.to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repo_spellings() {
        let want = Some(("octo".to_string(), "hello-world".to_string()));
        assert_eq!(parse_repo("octo/hello-world"), want);
        assert_eq!(parse_repo(" https://github.com/octo/hello-world/ "), want);
        assert_eq!(parse_repo("https://github.com/octo/hello-world.git"), want);
        assert_eq!(parse_repo("git@github.com:octo/hello-world.git"), want);
        assert_eq!(parse_repo("https://github.com/octo/hello-world/issues/3"), want);
        assert_eq!(parse_repo("octo"), None);
        assert_eq!(parse_repo("-x/repo"), None);
        assert_eq!(parse_repo("a b/repo"), None);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("headway-cfg-{}", std::process::id()));
        let path = dir.join("config.json");
        let mut c = Config::default();
        c.projects.push(Project {
            id: "octo/hello".into(),
            owner: "octo".into(),
            repo: "hello".into(),
            local_path: None,
            added_at: String::new(),
        });
        c.save(&path).unwrap();
        let back = Config::load(&path);
        assert_eq!(back.projects.len(), 1);
        assert_eq!(back.settings.sync_seconds, 30);
        assert!(back.resolve(Some("OCTO/Hello")).is_ok());
        assert!(back.resolve(None).is_ok());
        assert!(back.resolve(Some("nope/nope")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
