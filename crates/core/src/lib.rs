//! Headway core. The desktop app, the MCP server and the dev server are thin
//! shells around [`Service`].

pub mod auth;
pub mod calendar;
pub mod config;
pub mod error;
pub mod git;
pub mod github;
pub mod graph;
pub mod model;
pub mod proc;
pub mod service;
pub mod tasks;

pub use error::{Error, Result};
pub use service::Service;

pub const APP_NAME: &str = "Headway";
