use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// Every failure the core can report. `code()` is a stable machine name the UI
/// and the MCP server switch on; the `Display` text is meant for people.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("You are not signed in to GitHub.")]
    NotSignedIn,
    #[error("GitHub did not accept the sign-in. The token may have expired or been revoked.")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("GitHub's rate limit was reached. {0}")]
    RateLimited(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Could not reach GitHub: {0}")]
    Network(String),
    #[error("GitHub took too long to answer. Try again in a moment.")]
    Overloaded,
    #[error("GitHub error: {0}")]
    GitHub(String),
    #[error("{0}")]
    Git(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotSignedIn => "not_signed_in",
            Error::Unauthorized => "unauthorized",
            Error::Forbidden(_) => "forbidden",
            Error::NotFound(_) => "not_found",
            Error::RateLimited(_) => "rate_limited",
            Error::Invalid(_) => "invalid",
            Error::Network(_) => "network",
            Error::Overloaded => "overloaded",
            Error::GitHub(_) => "github",
            Error::Git(_) => "git",
            Error::Io(_) => "io",
            Error::Other(_) => "error",
        }
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Error::Invalid(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("Error", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::Network("the request timed out".into())
        } else if e.is_connect() {
            Error::Network("no connection".into())
        } else {
            // Never include the URL: it is noise for the user.
            Error::Network(e.without_url().to_string())
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Other(format!("unexpected data: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
