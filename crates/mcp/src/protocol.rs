//! JSON-RPC plumbing for both protocol generations.

use headway_core::Service;
use serde_json::{json, Value};

use crate::tools;

pub const SERVER_NAME: &str = "headway";
/// The handshake-free revision.
const MODERN: &str = "2026-07-28";
/// Revisions that open with `initialize`, newest first.
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

const INSTRUCTIONS: &str = "Headway tracks a small team's work. Every task is a GitHub issue in the project's repository, so a task number is the issue number (#12). \
Statuses: todo, in_progress, in_review, done, canceled. Priorities: urgent, high, medium, low. Due dates are YYYY-MM-DD. \
A project is written owner/repo; leave it out to use the project of the folder you are working in, or else the one selected in the Headway app. \
Look a task up (list_tasks or get_task) before changing it. When work on a task starts, set it to in_progress; when a pull request is opened for it, link the pull request and set in_review. \
A pull request that is linked to a task closes the task on GitHub when it is merged, so do not mark such a task done by hand.";

#[derive(Default)]
pub struct Session {
    /// Set once a client has opened with `initialize`.
    legacy: bool,
}

fn server_info() -> Value {
    json!({ "name": SERVER_NAME, "title": "Headway", "version": env!("CARGO_PKG_VERSION") })
}

pub fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut err = json!({ "code": code, "message": message });
    if let Some(d) = data {
        err["data"] = d;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": err })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// A result in the 2026-07-28 shape: `resultType` plus the server's identity.
fn modern(mut result: Value) -> Value {
    result["resultType"] = json!("complete");
    result["_meta"] = json!({ META_SERVER_INFO: server_info() });
    result
}

/// The answer to one incoming message, or `None` for notifications.
pub async fn handle(service: &Service, session: &mut Session, msg: Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?.to_string();
    // No id = a notification (initialized, cancelled ...): never answered.
    let id = msg.get("id").cloned().filter(|v| !v.is_null())?;
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let requested = params.pointer(&format!("/_meta/{}", META_VERSION.replace('/', "~1"))).and_then(Value::as_str);

    if method == "initialize" {
        session.legacy = true;
        let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
        // Answer with the client's revision if we speak it, otherwise our newest handshake revision.
        let version = LEGACY.iter().find(|v| **v == asked).copied().unwrap_or(LEGACY[0]);
        return Some(ok(
            id,
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": server_info(),
                "instructions": INSTRUCTIONS,
            }),
        ));
    }

    // A request that names its protocol version belongs to the handshake-free generation.
    let is_modern = match requested {
        Some(v) if v == MODERN => true,
        Some(v) if LEGACY.contains(&v) => false,
        Some(v) => {
            let data = json!({ "supported": [MODERN], "requested": v });
            return Some(error(id, -32022, "Unsupported protocol version", Some(data)));
        }
        None => false,
    };

    match method.as_str() {
        "server/discover" => {
            if !is_modern {
                // 2026-07-28 requires the version in `_meta`; without it this is not a valid probe.
                return Some(error(id, -32602, "Missing io.modelcontextprotocol/protocolVersion in params._meta", None));
            }
            let mut supported = vec![MODERN];
            supported.extend_from_slice(LEGACY);
            Some(ok(
                id,
                modern(json!({
                    "supportedVersions": supported,
                    "capabilities": { "tools": {} },
                    "instructions": INSTRUCTIONS,
                    "ttlMs": 3_600_000,
                    "cacheScope": "public",
                })),
            ))
        }
        "ping" => Some(ok(id, json!({}))),
        "tools/list" => {
            let list = json!({ "tools": tools::definitions() });
            Some(ok(id, if is_modern { modern(with_cache(list)) } else { list }))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            if !tools::exists(name) {
                return Some(error(id, -32602, &format!("Unknown tool: {name}"), None));
            }
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            let result = tools::call(service, name, args).await;
            Some(ok(id, if is_modern { modern(result) } else { result }))
        }
        // Some older clients ask for these even when the capability is not declared.
        "resources/list" if session.legacy => Some(ok(id, json!({ "resources": [] }))),
        "prompts/list" if session.legacy => Some(ok(id, json!({ "prompts": [] }))),
        _ => Some(error(id, -32601, &format!("Method not found: {method}"), None)),
    }
}

/// The tool list is the same for everyone and only changes with a new build.
fn with_cache(mut result: Value) -> Value {
    result["ttlMs"] = json!(300_000);
    result["cacheScope"] = json!("public");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> Service {
        let dir = std::env::temp_dir().join(format!("headway-mcp-test-{}", std::process::id()));
        Service::with_config(dir.join("config.json"), true)
    }

    async fn send(session: &mut Session, msg: Value) -> Option<Value> {
        handle(&service(), session, msg).await
    }

    #[tokio::test]
    async fn legacy_handshake_then_tools() {
        let mut s = Session::default();
        let init = send(&mut s, json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } } })).await.unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "headway");
        assert!(init["result"]["capabilities"]["tools"].is_object());
        assert!(init["result"].get("resultType").is_none());

        // The follow-up notification gets no answer.
        assert!(send(&mut s, json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await.is_none());

        let list = send(&mut s, json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await.unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert!(tools.len() >= 10);
        for t in tools {
            assert!(t["name"].is_string() && t["description"].is_string());
            assert_eq!(t["inputSchema"]["type"], "object");
        }
        assert!(list["result"].get("resultType").is_none());

        let ping = send(&mut s, json!({ "jsonrpc": "2.0", "id": "p", "method": "ping" })).await.unwrap();
        assert_eq!(ping["result"], json!({}));
        assert_eq!(ping["id"], "p");
    }

    #[tokio::test]
    async fn unknown_legacy_version_gets_our_newest_handshake_version() {
        let mut s = Session::default();
        let init = send(&mut s, json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } })).await.unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    }

    #[tokio::test]
    async fn modern_discover_and_stateless_calls() {
        let mut s = Session::default();
        let meta = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} });
        let d = send(&mut s, json!({ "jsonrpc": "2.0", "id": "discover-1", "method": "server/discover", "params": { "_meta": meta } })).await.unwrap();
        let r = &d["result"];
        assert_eq!(r["resultType"], "complete");
        assert_eq!(r["supportedVersions"][0], "2026-07-28");
        assert!(r["capabilities"]["tools"].is_object());
        assert_eq!(r["cacheScope"], "public");
        assert!(r["ttlMs"].is_u64());
        assert_eq!(r["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "headway");

        let list = send(&mut s, json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": { "_meta": meta } })).await.unwrap();
        assert_eq!(list["result"]["resultType"], "complete");
        assert_eq!(list["result"]["cacheScope"], "public");
        assert!(list["result"]["tools"].as_array().unwrap().len() >= 10);

        // No project is configured in the test config: the tool reports it as a tool error, not a protocol error.
        let call = send(&mut s, json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "list_tasks", "arguments": {}, "_meta": meta } })).await.unwrap();
        assert_eq!(call["result"]["resultType"], "complete");
        assert_eq!(call["result"]["isError"], true);
        assert!(call["result"]["content"][0]["text"].as_str().unwrap().contains("No project"));
    }

    #[tokio::test]
    async fn protocol_errors_use_the_documented_codes() {
        let mut s = Session::default();
        let bad_version = send(&mut s, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "2030-01-01" } } })).await.unwrap();
        assert_eq!(bad_version["error"]["code"], -32022);
        assert_eq!(bad_version["error"]["data"]["requested"], "2030-01-01");

        let probe_without_version = send(&mut s, json!({ "jsonrpc": "2.0", "id": 2, "method": "server/discover" })).await.unwrap();
        assert_eq!(probe_without_version["error"]["code"], -32602);

        let unknown_tool = send(&mut s, json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "nope" } })).await.unwrap();
        assert_eq!(unknown_tool["error"]["code"], -32602);
        assert_eq!(unknown_tool["error"]["message"], "Unknown tool: nope");

        let unknown_method = send(&mut s, json!({ "jsonrpc": "2.0", "id": 4, "method": "does/not/exist" })).await.unwrap();
        assert_eq!(unknown_method["error"]["code"], -32601);
    }
}
