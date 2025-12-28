use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use crate::{api, auth, config, logging, pages, realtime, template, ws};

const RELOAD_SCRIPT: &str = r#"<script>
(function(){let m=0;setInterval(async()=>{const r=await fetch('/__dev/mtime');const t=await r.text();if(m&&t!==m)location.reload();m=t;},500);})();
</script>"#;

pub fn handle(mut stream: TcpStream) {
    let mut buffer = [0; 8192];
    let n = stream.read(&mut buffer).unwrap_or(0);
    let request = String::from_utf8_lossy(&buffer[..n]);

    let (method, path, query, headers, body) = parse_request(&request);

    if is_websocket(&headers) && path == "/realtime" {
        if !authorize_realtime(&headers, &query) {
            let _ = stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n");
            return;
        }
        if ws::handshake(&mut stream, &headers).is_ok() {
            realtime::register(stream);
        }
        return;
    }

    let (status, content, content_type, cors) = route(&method, &path, &headers, &body);
    logging::info("http", &format!("{} {} -> {}", method, path, status));

    let mut response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: same-origin\r\n",
        status, content_type, content.len()
    );
    if cors {
        // CORS origin is configurable via CORS_ORIGIN env var (defaults to "*" for development)
        // Production deployments should set a specific origin (e.g., "https://example.com")
        let origin = config::cors_origin();
        response.push_str(&format!(
            "Access-Control-Allow-Origin: {}\r\nAccess-Control-Allow-Headers: Content-Type, Authorization, X-Requested-With\r\nAccess-Control-Allow-Methods: GET,POST,PUT,DELETE,OPTIONS\r\n",
            origin
        ));
    }
    response.push_str("\r\n");

    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(&content);
}

fn parse_request(req: &str) -> (String, String, String, HashMap<String, String>, String) {
    let mut lines = req.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let raw_path = parts.next().unwrap_or("/");
    let (path, query) = raw_path.split_once('?').unwrap_or((raw_path, ""));
    let path = path.to_string();
    let query = query.to_string();

    let mut headers = HashMap::new();
    let mut body_start = false;
    let mut body = String::new();

    for line in lines {
        if line.is_empty() {
            body_start = true;
            continue;
        }
        if body_start {
            body.push_str(line);
        } else if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }

    (method, path, query, headers, body)
}

fn route(method: &str, path: &str, headers: &HashMap<String, String>, body: &str) -> (&'static str, Vec<u8>, &'static str, bool) {
    // Handle OPTIONS for CORS
    if method == "OPTIONS" {
        return ("200 OK", Vec::new(), "text/plain", true);
    }

    // API routes
    if path.starts_with("/api/") {
        let req = api::Request {
            method: method.to_string(),
            path: path.to_string(),
            headers: headers.clone(),
            body: body.to_string(),
        };
        let res = api::handle(&req);
        let status = match res.status {
            200 => "200 OK",
            201 => "201 Created",
            400 => "400 Bad Request",
            401 => "401 Unauthorized",
            404 => "404 Not Found",
            _ => "500 Internal Server Error",
        };
        return (status, res.body.into_bytes(), "application/json", true);
    }

    // Page routes
    match path {
        "/__dev/mtime" if config::hot_reload() => get_mtime(),
        "/__dev/mtime" => ("404 Not Found", b"Not Found".to_vec(), "text/plain", false),
        "/" | "/index.html" => render_page(pages::index().render()),
        "/_admin" => render_admin(),
        p if p.starts_with("/docs") => render_page(render_docs(p)),
        p if p.starts_with("/projects/") => serve_project(p),
        _ => serve_file(path),
    }
}

fn render_page(mut html: String) -> (&'static str, Vec<u8>, &'static str, bool) {
    html = ensure_doctype(html);
    if config::hot_reload() {
        html = html.replace("</body>", &format!("{}</body>", RELOAD_SCRIPT));
    }
    ("200 OK", html.into_bytes(), "text/html", false)
}

fn render_docs(path: &str) -> String {
    let slug = path.trim_start_matches("/docs").trim_start_matches('/');
    pages::docs(slug).render()
}

fn render_admin() -> (&'static str, Vec<u8>, &'static str, bool) {
    let mut ctx = pages::settings_context("Admin");
    ctx.set("body_class", "h-screen overflow-hidden");
    let mut html = ensure_doctype(template::render(&template::load("admin.html"), &ctx));
    if config::hot_reload() {
        html = html.replace("</body>", &format!("{}</body>", RELOAD_SCRIPT));
    }
    ("200 OK", html.into_bytes(), "text/html", false)
}

fn serve_project(path: &str) -> (&'static str, Vec<u8>, &'static str, bool) {
    let rel_path = path.trim_start_matches("/projects/").trim_start_matches('/');
    let mut file_path = config::root_dir().join("projects").join(rel_path);

    // If directory, look for index.html
    if file_path.is_dir() {
        file_path = file_path.join("index.html");
    }

    if let Ok(content) = fs::read(&file_path) {
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "html" => {
                let html_str = String::from_utf8_lossy(&content).to_string();
                let mut ctx = pages::settings_context("Project");
                // Add defaults for footer/nav
                ctx.set("name", "Ólafur Búi Ólafsson");
                ctx.set("location", "Akureyri, Iceland");
                
                let rendered = template::render(&html_str, &ctx);
                let (status, bytes, ct, cors) = render_page(rendered);
                (status, bytes, ct, cors)
            }
            "css" => ("200 OK", content, "text/css", false),
            "js" => ("200 OK", content, "application/javascript", false),
            "png" => ("200 OK", content, "image/png", false),
            "jpg" | "jpeg" => ("200 OK", content, "image/jpeg", false),
            "svg" => ("200 OK", content, "image/svg+xml", false),
            _ => ("200 OK", content, "text/plain", false),
        }
    } else {
        ("404 Not Found", b"Not Found".to_vec(), "text/plain", false)
    }
}

fn get_mtime() -> (&'static str, Vec<u8>, &'static str, bool) {
    fn scan(dir: &str, max: &mut u64) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() { scan(&path.to_string_lossy(), max); }
                else if let Ok(m) = entry.metadata() {
                    if let Ok(t) = m.modified() {
                        if let Ok(d) = t.duration_since(UNIX_EPOCH) {
                            *max = (*max).max(d.as_millis() as u64);
                        }
                    }
                }
            }
        }
    }
    let mut max = 0u64;
    scan(&config::public_dir().to_string_lossy(), &mut max);
    ("200 OK", max.to_string().into_bytes(), "text/plain", false)
}

fn serve_file(path: &str) -> (&'static str, Vec<u8>, &'static str, bool) {
    let file_path = match safe_public_path(path) {
        Some(p) => p,
        None => return ("404 Not Found", b"Not Found".to_vec(), "text/plain", false),
    };

    if let Ok(mut content) = fs::read(&file_path) {
        let ct = match Path::new(path).extension().and_then(|e| e.to_str()) {
            Some("html") => { content = inject_reload(content); "text/html" }
            Some("css") => "text/css",
            Some("js") => "application/javascript",
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("svg") => "image/svg+xml",
            Some("json") => "application/json",
            Some("woff2") => "font/woff2",
            Some("ico") => "image/x-icon",
            _ => "text/plain",
        };
        ("200 OK", content, ct, false)
    } else {
        ("404 Not Found", b"Not Found".to_vec(), "text/plain", false)
    }
}

fn inject_reload(content: Vec<u8>) -> Vec<u8> {
    String::from_utf8(content)
        .map(ensure_doctype)
        .map(|h| {
            if config::hot_reload() {
                h.replace("</body>", &format!("{}</body>", RELOAD_SCRIPT))
            } else {
                h
            }
        })
        .map(|s| s.into_bytes())
        .unwrap_or_default()
}

fn ensure_doctype(html: String) -> String {
    let trimmed = html.trim_start();
    if trimmed.to_lowercase().starts_with("<!doctype html>") {
        html
    } else {
        format!("<!DOCTYPE html>\n{}", html)
    }
}

fn safe_public_path(path: &str) -> Option<PathBuf> {
    let clean = if path.starts_with('/') { path } else { return None };
    let rel = Path::new(clean.trim_start_matches('/'));
    for component in rel.components() {
        if matches!(component, std::path::Component::ParentDir | std::path::Component::Prefix(_)) {
            return None;
        }
    }
    Some(config::public_dir().join(rel))
}

fn is_websocket(headers: &HashMap<String, String>) -> bool {
    headers.get("upgrade").map(|v| v.eq_ignore_ascii_case("websocket")).unwrap_or(false)
}

fn authorize_realtime(headers: &HashMap<String, String>, query: &str) -> bool {
    let token = query_param(query, "token")
        .or_else(|| headers.get("authorization").map(|h| h.trim_start_matches("Bearer ").to_string()))
        .unwrap_or_default();
    auth::is_admin(&token)
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').filter_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next()?;
        let v = parts.next().unwrap_or("");
        if k == key {
            Some(url_decode(v))
        } else {
            None
        }
    }).next()
}

fn url_decode(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            let hex = format!("{}{}", h1, h2);
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                out.push(byte as char);
            }
        } else if c == '+' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}
