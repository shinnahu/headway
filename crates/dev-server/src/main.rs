//! Development helper, not part of the shipped app.
//!
//! Serves the `ui/` folder and exposes the core at `POST /api/<method>`, so the
//! screens can be opened in an ordinary browser with real data.
//!
//! It listens on 127.0.0.1 only and refuses requests that a web page from
//! another origin could send (it checks Host and Origin and requires a custom
//! header, which browsers will not send cross-origin without a preflight that
//! this server never approves).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use headway_core::Service;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header")
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn request_header<'a>(req: &'a Request, name: &'static str) -> Option<&'a str> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn respond_json(req: Request, status: u16, body: Value) {
    let resp = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"));
    let _ = req.respond(resp);
}

fn serve_file(req: Request, ui: &Path) {
    let url = req.url().split(['?', '#']).next().unwrap_or("/").to_string();
    let rel = url.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    // Never leave the ui folder.
    if rel.split(['/', '\\']).any(|seg| seg == ".." || seg.contains(':')) {
        let _ = req.respond(Response::from_string("not found").with_status_code(404));
        return;
    }
    let path = ui.join(rel);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let resp = Response::from_data(bytes)
                .with_header(header("Content-Type", content_type(&path)))
                .with_header(header("Cache-Control", "no-store"));
            let _ = req.respond(resp);
        }
        Err(_) => {
            let _ = req.respond(Response::from_string("not found").with_status_code(404));
        }
    }
}

fn handle(mut req: Request, service: &Service, rt: &tokio::runtime::Runtime, ui: &Path, origin: &str, host: &str) {
    // Only requests addressed to this very server (blocks DNS-rebinding tricks).
    let host_ok = request_header(&req, "Host").is_some_and(|h| h == host || h == host.replace("127.0.0.1", "localhost"));
    if !host_ok {
        respond_json(req, 403, json!({ "ok": false, "error": { "code": "forbidden", "message": "wrong host" } }));
        return;
    }
    let url = req.url().to_string();
    if let Some(method) = url.strip_prefix("/api/") {
        let method = method.split('?').next().unwrap_or("").to_string();
        let same_origin = request_header(&req, "Origin").is_none_or(|o| o == origin || o == origin.replace("127.0.0.1", "localhost"));
        let marked = request_header(&req, "X-Headway").is_some();
        if *req.method() != Method::Post || !same_origin || !marked {
            respond_json(req, 403, json!({ "ok": false, "error": { "code": "forbidden", "message": "not allowed" } }));
            return;
        }
        let mut body = String::new();
        if req.as_reader().take(4 * 1024 * 1024).read_to_string(&mut body).is_err() {
            respond_json(req, 400, json!({ "ok": false, "error": { "code": "invalid", "message": "unreadable body" } }));
            return;
        }
        let params: Value = if body.trim().is_empty() { Value::Null } else { serde_json::from_str(&body).unwrap_or(Value::Null) };
        let started = std::time::Instant::now();
        let result = rt.block_on(service.call(&method, params));
        let ms = started.elapsed().as_millis();
        match result {
            Ok(data) => {
                println!("{method:<22} ok   {ms:>5} ms");
                respond_json(req, 200, json!({ "ok": true, "data": data }));
            }
            Err(e) => {
                println!("{method:<22} {:<4} {ms:>5} ms  {e}", e.code());
                respond_json(req, 200, json!({ "ok": false, "error": e }));
            }
        }
        return;
    }
    if *req.method() == Method::Get {
        serve_file(req, ui);
    } else {
        let _ = req.respond(Response::from_string("method not allowed").with_status_code(405));
    }
}

fn main() {
    let mut port: u16 = 4317;
    let mut ui = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("ui");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = args.next().and_then(|p| p.parse().ok()).unwrap_or(port),
            "--ui" => ui = args.next().map(PathBuf::from).unwrap_or(ui),
            other => eprintln!("ignoring unknown argument: {other}"),
        }
    }
    let host = format!("127.0.0.1:{port}");
    let origin = format!("http://{host}");
    let server = match Server::http(&host) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("could not listen on {host}: {e}");
            std::process::exit(1);
        }
    };
    let rt = Arc::new(tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime"));
    let service = Arc::new(Service::new());
    println!("Headway dev server ready at {origin}  (ui: {})", ui.display());

    let mut workers = Vec::new();
    for _ in 0..6 {
        let (server, rt, service, ui, origin, host) = (server.clone(), rt.clone(), service.clone(), ui.clone(), origin.clone(), host.clone());
        workers.push(std::thread::spawn(move || {
            while let Ok(req) = server.recv() {
                handle(req, &service, &rt, &ui, &origin, &host);
            }
        }));
    }
    for w in workers {
        let _ = w.join();
    }
}
