//! HTTPS reverse proxy for external routes.
use std::collections::HashMap;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use rustls::ServerConfig;
use rustls::pki_types::CertificateDer;

use crate::{auth, config, db, ports};

const BASE_HOSTS: [&str; 2] = ["olibuijr.com", "www.olibuijr.com"];

pub fn run_proxy() {
    let tls_config = load_tls_config();
    let tls_config = Arc::new(tls_config);

    let http_port = env_port("RPW_HTTP_PORT", 80);
    let https_port = env_port("RPW_HTTPS_PORT", 443);
    let http_listener = TcpListener::bind(("0.0.0.0", http_port)).unwrap();
    let https_listener = TcpListener::bind(("0.0.0.0", https_port)).unwrap();

    std::thread::spawn(move || {
        for stream in http_listener.incoming().flatten() {
            handle_http(stream);
        }
    });

    for stream in https_listener.incoming().flatten() {
        let cfg = tls_config.clone();
        std::thread::spawn(move || {
            handle_https(stream, cfg);
        });
    }
}

fn handle_http(mut stream: TcpStream) {
    let (_request, _req) = match read_request(&mut stream) {
        Some(v) => v,
        None => return,
    };
    let host = extract_host(&_req.headers).unwrap_or_default();
    let target_host = if host.eq_ignore_ascii_case("olibuijr.com") {
        "www.olibuijr.com"
    } else {
        host.as_str()
    };
    let location = format!("https://{}{}", target_host, _req.path);
    let response = format!(
        "HTTP/1.1 301 Moved Permanently\r\nLocation: {}\r\nContent-Length: 0\r\n\r\n",
        location
    );
    let _ = stream.write_all(response.as_bytes());
}

fn env_port(key: &str, default_port: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(default_port)
}

fn handle_https(stream: TcpStream, tls_config: Arc<ServerConfig>) {
    let tls = rustls::ServerConnection::new(tls_config).ok();
    let tls = match tls {
        Some(t) => t,
        None => return,
    };
    let mut tls_stream = rustls::StreamOwned::new(tls, stream);
    let (raw, req) = match read_request(&mut tls_stream) {
        Some(v) => v,
        None => return,
    };

    let host = extract_host(&req.headers).unwrap_or_default();
    if host.eq_ignore_ascii_case("olibuijr.com") {
        let location = format!("https://www.olibuijr.com{}", req.path);
        let response = format!(
            "HTTP/1.1 301 Moved Permanently\r\nLocation: {}\r\nContent-Length: 0\r\n\r\n",
            location
        );
        let _ = tls_stream.write_all(response.as_bytes());
        return;
    }
    match route_target(&host) {
        Route::Base => {
            let _ = proxy_to("127.0.0.1", 3460, &raw, &mut tls_stream);
        }
        Route::Project { host, port } => {
            if !authorize(&req.headers) {
                let _ = respond_unauthorized(&mut tls_stream);
                return;
            }
            let _ = proxy_to(&host, port, &raw, &mut tls_stream);
        }
        Route::NotFound => {
            let _ = respond_not_found(&mut tls_stream);
        }
    }
}

fn route_target(host: &str) -> Route {
    if BASE_HOSTS.iter().any(|h| h.eq_ignore_ascii_case(host)) {
        return Route::Base;
    }

    if let Some(project) = host.strip_suffix(".olibuijr.com") {
        if let Some(dev_project) = project.strip_prefix("dev-") {
            if let Some((ip, port)) = project_target(dev_project, "dev_port", "dev_ip_base") {
                return Route::Project { host: ip, port };
            }
        } else if let Some((ip, port)) = project_target(project, "prod_port", "prod_ip_base") {
            return Route::Project { host: ip, port };
        }
    }

    Route::NotFound
}

fn project_target(project: &str, port_key: &str, base_key: &str) -> Option<(String, u16)> {
    let docs = db::get().find_all("_ports");
    let settings = db::get().find_all("_settings");
    let base = settings.first()
        .and_then(|doc| doc.get(base_key))
        .and_then(|v| v.as_str())
        .unwrap_or("10.35.0.");
    for doc in docs {
        let name = doc.get("project").and_then(|v| v.as_str()).unwrap_or("");
        if name == project {
            if let Some(db::Value::Int(port)) = doc.get(port_key) {
                if *port > 0 && *port <= u16::MAX as i64 {
                    if let Some(ip) = ports::ip_from_port(base, *port as u16) {
                        return Some((ip, 80));
                    }
                }
            }
        }
    }
    None
}

fn authorize(headers: &HashMap<String, String>) -> bool {
    let token = headers.get("authorization")
        .map(|h| h.trim_start_matches("Bearer ").to_string())
        .or_else(|| headers.get("cookie")
            .and_then(|c| c.split(';').find(|p| p.trim().starts_with("token=")))
            .map(|p| p.trim().trim_start_matches("token=").to_string()))
        .unwrap_or_default();
    auth::is_admin(&token)
}

fn respond_unauthorized(stream: &mut dyn Write) -> std::io::Result<()> {
    stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n")
}

fn respond_not_found(stream: &mut dyn Write) -> std::io::Result<()> {
    stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
}

fn proxy_to(host: &str, port: u16, raw: &[u8], client: &mut dyn Write) -> std::io::Result<()> {
    let mut upstream = TcpStream::connect((host, port))?;
    upstream.write_all(raw)?;
    let mut buf = Vec::new();
    upstream.read_to_end(&mut buf)?;
    client.write_all(&buf)?;
    Ok(())
}


fn read_request(stream: &mut dyn Read) -> Option<(Vec<u8>, ParsedRequest)> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp).ok()?;
        if n == 0 { break; }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    if buf.is_empty() {
        return None;
    }
    let req = parse_request_bytes(&buf)?;
    let content_len = req.headers.get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let have = buf.len().saturating_sub(req.body_offset);
    if content_len > have {
        let mut remaining = content_len - have;
        while remaining > 0 {
            let n = stream.read(&mut tmp).ok()?;
            if n == 0 { break; }
            buf.extend_from_slice(&tmp[..n]);
            remaining = remaining.saturating_sub(n);
        }
    }
    Some((buf, req))
}

fn parse_request_bytes(buf: &[u8]) -> Option<ParsedRequest> {
    let text = String::from_utf8_lossy(buf);
    let mut lines = text.lines();
    let first = lines.next()?;
    let mut parts = first.split_whitespace();
    let path = parts.next()?.to_string();
    let mut headers = HashMap::new();
    let mut offset = 0usize;
    for (i, b) in buf.windows(4).enumerate() {
        if b == b"\r\n\r\n" {
            offset = i + 4;
            break;
        }
    }
    for line in lines {
        if line.is_empty() { break; }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    Some(ParsedRequest {
        path,
        headers,
        body_offset: offset,
    })
}

fn extract_host(headers: &HashMap<String, String>) -> Option<String> {
    headers.get("host").map(|h| h.split(':').next().unwrap_or(h).to_string())
}

fn load_tls_config() -> ServerConfig {
    let cert_path = config::root_dir().join("certs/server.crt");
    let key_path = config::root_dir().join("certs/server.key");

    let cert_file = std::fs::File::open(cert_path).expect("missing certs/server.crt");
    let key_file = std::fs::File::open(key_path).expect("missing certs/server.key");
    let mut cert_reader = std::io::BufReader::new(cert_file);
    let mut key_reader = std::io::BufReader::new(key_file);

    let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
        .filter_map(Result::ok)
        .collect();
    let key = rustls_pemfile::private_key(&mut key_reader)
        .ok()
        .flatten()
        .expect("invalid private key");

    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("invalid cert/key")
}

enum Route {
    Base,
    Project { host: String, port: u16 },
    NotFound,
}

struct ParsedRequest {
    path: String,
    headers: HashMap<String, String>,
    body_offset: usize,
}
