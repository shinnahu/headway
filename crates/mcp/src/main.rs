//! Headway's MCP server: lets Claude read and update the team's tasks.
//!
//! It speaks the Model Context Protocol over stdio (one JSON-RPC message per
//! line) and offers tools only. Two generations of the protocol are served:
//!
//! * up to 2025-11-25: the client opens with `initialize`;
//! * 2026-07-28: no handshake; the client probes with `server/discover` and
//!   every request carries its protocol version in `params._meta`.
//!
//! It uses the same sign-in and project list as the desktop app, read from the
//! app's config file on every call.

mod format;
mod protocol;
mod tools;

use headway_core::Service;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() {
    let service = Service::following_config_file();
    let mut session = protocol::Session::default();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    // stdout carries protocol messages only; anything for a human goes to stderr.
    while let Ok(Some(line)) = lines.next_line().await {
        // Some shells put a byte-order mark in front of the first line they pipe in.
        let line = line.trim().trim_start_matches('\u{feff}').trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => protocol::handle(&service, &mut session, msg).await,
            Err(_) => Some(protocol::error(Value::Null, -32700, "Parse error", None)),
        };
        if let Some(reply) = reply {
            let mut text = reply.to_string();
            text.push('\n');
            if stdout.write_all(text.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
                break;
            }
        }
    }
}
