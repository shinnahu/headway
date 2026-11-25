//! Signing in to GitHub without a server of our own. Three ways:
//!
//! 1. the GitHub CLI's existing session (`gh auth token`), nothing stored by us;
//! 2. GitHub's device flow, with the team's own OAuth App client id;
//! 3. a personal access token the user pastes.
//!
//! Tokens from 2 and 3 are kept in the OS credential store (Windows Credential
//! Manager), never in the config file and never sent to the UI.

use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{proc, Error, Result};

const KEYRING_SERVICE: &str = "Headway";
/// `repo` is what GitHub requires to read and write issues and pull requests
/// in private repositories; there is no narrower OAuth scope for that.
pub const DEVICE_SCOPE: &str = "repo read:org";

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct GhAccount {
    pub login: String,
    pub active: bool,
}

pub fn valid_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 60
        && !login.starts_with('-')
        && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Accounts the GitHub CLI is signed in to on github.com (empty when `gh` is
/// missing or signed out).
pub async fn gh_accounts() -> Vec<GhAccount> {
    let t = Duration::from_secs(15);
    if let Ok(out) = proc::run("gh", &["auth", "status", "--json", "hosts"], None, &[], t).await {
        if let Ok(v) = serde_json::from_str::<Value>(out.stdout.trim()) {
            if let Some(list) = v.pointer("/hosts/github.com").and_then(Value::as_array) {
                return list
                    .iter()
                    .filter(|a| a.get("state").and_then(Value::as_str) == Some("success"))
                    .filter_map(|a| {
                        Some(GhAccount {
                            login: a.get("login")?.as_str()?.to_string(),
                            active: a.get("active").and_then(Value::as_bool).unwrap_or(false),
                        })
                    })
                    .collect();
            }
        }
    }
    // Older gh versions have no --json for this command.
    match proc::run("gh", &["auth", "status", "--hostname", "github.com"], None, &[], t).await {
        Ok(out) => parse_gh_status_text(&format!("{}\n{}", out.stdout, out.stderr)),
        Err(_) => Vec::new(),
    }
}

fn parse_gh_status_text(text: &str) -> Vec<GhAccount> {
    let re = Regex::new(r"Logged in to github\.com (?:account|as) ([A-Za-z0-9_-]+)").expect("valid regex");
    let mut accounts: Vec<GhAccount> = Vec::new();
    let mut saw_active_line = false;
    for line in text.lines() {
        if let Some(c) = re.captures(line) {
            accounts.push(GhAccount { login: c[1].to_string(), active: false });
        } else if line.contains("Active account:") {
            saw_active_line = true;
            if let Some(last) = accounts.last_mut() {
                last.active = line.contains("true");
            }
        }
    }
    if !saw_active_line {
        if let Some(first) = accounts.first_mut() {
            first.active = true;
        }
    }
    accounts
}

pub async fn gh_token(login: &str) -> Result<String> {
    if !valid_login(login) {
        return Err(Error::invalid("That is not a valid GitHub user name."));
    }
    let out = proc::run(
        "gh",
        &["auth", "token", "--hostname", "github.com", "--user", login],
        None,
        &[],
        Duration::from_secs(15),
    )
    .await?;
    let token = out.stdout.trim();
    if !out.ok || token.is_empty() {
        return Err(Error::Other(format!(
            "The GitHub CLI has no session for {login}. Run `gh auth login`, or sign in another way."
        )));
    }
    Ok(token.to_string())
}

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, account).map_err(cred_err)
}

fn cred_err(e: keyring::Error) -> Error {
    Error::Other(format!("The system credential store refused: {e}"))
}

/// Keeps a secret in the OS credential store under `account`
/// ("github:<login>" for tokens, "calendar:<id>" for calendar links).
pub fn store_secret(account: &str, secret: &str) -> Result<()> {
    entry(account)?.set_password(secret).map_err(cred_err)
}

pub fn load_secret(account: &str) -> Result<Option<String>> {
    match entry(account)?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(cred_err(e)),
    }
}

pub fn delete_secret(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(cred_err(e)),
    }
}

pub fn store_token(login: &str, token: &str) -> Result<()> {
    store_secret(&format!("github:{login}"), token)
}

pub fn load_token(login: &str) -> Result<Option<String>> {
    load_secret(&format!("github:{login}"))
}

pub fn delete_token(login: &str) -> Result<()> {
    delete_secret(&format!("github:{login}"))
}

/// A device-flow sign-in that is waiting for the user to type the code on
/// github.com/login/device.
pub struct DeviceSession {
    pub client_id: String,
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: Instant,
    pub interval: u64,
}

pub enum DevicePoll {
    /// The user has not entered the code yet.
    Pending,
    /// We asked too fast; GitHub wants 5 more seconds between polls.
    SlowDown,
    Token(String),
    Expired,
    Denied,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

fn plain_http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("headway-app/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()?)
}

pub async fn device_start(client_id: &str) -> Result<DeviceSession> {
    let client_id = client_id.trim();
    if client_id.is_empty() {
        return Err(Error::invalid("Add your GitHub OAuth App's Client ID in Settings first."));
    }
    let resp = plain_http()?
        .post("https://github.com/login/device/code")
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[("client_id", client_id), ("scope", DEVICE_SCOPE)])
        .send()
        .await?;
    let status = resp.status();
    let v: Value = serde_json::from_str(&resp.text().await?).unwrap_or(Value::Null);
    if let Some(code) = v.get("error").and_then(Value::as_str) {
        return Err(device_error(code, &v));
    }
    if !status.is_success() {
        return Err(Error::GitHub(format!("the device sign-in could not start ({status})")));
    }
    let r: DeviceCodeResponse = serde_json::from_value(v)?;
    Ok(DeviceSession {
        client_id: client_id.to_string(),
        device_code: r.device_code,
        user_code: r.user_code,
        verification_uri: r.verification_uri,
        expires_at: Instant::now() + Duration::from_secs(r.expires_in),
        interval: r.interval.max(1),
    })
}

pub async fn device_poll(session: &DeviceSession) -> Result<DevicePoll> {
    if Instant::now() >= session.expires_at {
        return Ok(DevicePoll::Expired);
    }
    let resp = plain_http()?
        .post("https://github.com/login/oauth/access_token")
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("client_id", session.client_id.as_str()),
            ("device_code", session.device_code.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await?;
    let v: Value = serde_json::from_str(&resp.text().await?).unwrap_or(Value::Null);
    if let Some(token) = v.get("access_token").and_then(Value::as_str) {
        return Ok(DevicePoll::Token(token.to_string()));
    }
    match v.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => Ok(DevicePoll::Pending),
        Some("slow_down") => Ok(DevicePoll::SlowDown),
        // GitHub's docs spell this one both ways.
        Some("expired_token") | Some("token_expired") => Ok(DevicePoll::Expired),
        Some("access_denied") => Ok(DevicePoll::Denied),
        Some(code) => Err(device_error(code, &v)),
        None => Err(Error::GitHub("unexpected answer while waiting for the sign-in".into())),
    }
}

fn device_error(code: &str, v: &Value) -> Error {
    match code {
        "device_flow_disabled" => Error::invalid(
            "Device flow is not enabled for this OAuth App. Enable it in the app's settings on GitHub.",
        ),
        "incorrect_client_credentials" | "Not Found" => {
            Error::invalid("GitHub does not recognise this Client ID.")
        }
        _ => {
            let desc = v.get("error_description").and_then(Value::as_str).unwrap_or(code);
            Error::GitHub(desc.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gh_text_status_with_two_accounts() {
        let text = "github.com\n  ✓ Logged in to github.com account alice (keyring)\n  - Active account: true\n  ✓ Logged in to github.com account bob-co (keyring)\n  - Active account: false\n";
        assert_eq!(
            parse_gh_status_text(text),
            vec![
                GhAccount { login: "alice".into(), active: true },
                GhAccount { login: "bob-co".into(), active: false },
            ]
        );
    }

    #[test]
    fn parses_old_gh_text_status() {
        let text = "github.com\n  ✓ Logged in to github.com as alice (oauth_token)\n";
        assert_eq!(parse_gh_status_text(text), vec![GhAccount { login: "alice".into(), active: true }]);
        assert!(parse_gh_status_text("You are not logged into any GitHub hosts.").is_empty());
    }

    #[test]
    fn rejects_logins_that_could_be_flags() {
        assert!(valid_login("octo-cat_1"));
        assert!(!valid_login("-rf"));
        assert!(!valid_login("a b"));
        assert!(!valid_login(""));
    }
}
