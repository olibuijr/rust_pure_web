//! Zero-dependency integration test runner
//! Run after deployment: ./target/release/healthcheck [host:port]

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

const DEFAULT_HOST: &str = "localhost:3460";

fn main() {
    let host = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_HOST.to_string());
    let env = load_env();
    println!("🧪 Running health checks against {}\n", host);

    let mut passed = 0;
    let mut failed = 0;
    let mut token = String::new();

    // ── Static Pages ─────────────────────────────────────────────────────────

    test(&host, "GET / returns 200", || {
        let res = http_get(&host, "/")?;
        assert_status(&res, 200)?;
        assert_contains(&res, "<!DOCTYPE html>")?;
        Ok(())
    }, &mut passed, &mut failed);

    test(&host, "GET /_admin returns 200", || {
        let res = http_get(&host, "/_admin")?;
        assert_status(&res, 200)?;
        assert_contains(&res, "<!DOCTYPE html>")?;
        assert_contains(&res, "admin")?;
        assert_contains(&res, "renderAssistantMessage")?;
        Ok(())
    }, &mut passed, &mut failed);

    test(&host, "GET /styles.css returns 200", || {
        let res = http_get(&host, "/styles.css")?;
        assert_status(&res, 200)?;
        Ok(())
    }, &mut passed, &mut failed);

    test(&host, "GET /nonexistent returns 404", || {
        let res = http_get(&host, "/nonexistent-page-12345")?;
        assert_status(&res, 404)?;
        Ok(())
    }, &mut passed, &mut failed);

    // ── Auth API ─────────────────────────────────────────────────────────────

    test(&host, "GET /api/auth/me without token returns 401", || {
        let res = http_get(&host, "/api/auth/me")?;
        assert_status(&res, 401)?;
        assert_json_has(&res, "error")?;
        Ok(())
    }, &mut passed, &mut failed);

    test(&host, "POST /api/auth/login with bad creds returns 400", || {
        let res = http_post(&host, "/api/auth/login", r#"{"email":"bad@test.com","password":"wrongpass"}"#)?;
        assert_status(&res, 400)?;
        Ok(())
    }, &mut passed, &mut failed);

    // Test login with env credentials if available
    let email = env.get("ADMIN_EMAIL").cloned().unwrap_or_default();
    let password = env.get("ADMIN_PASSWORD").cloned().unwrap_or_default();

    if !email.is_empty() && !password.is_empty() {
        let h = host.clone();
        let e = email.clone();
        let p = password.clone();
        test(&host, "POST /api/auth/login with valid creds returns 200", move || {
            let body = format!(r#"{{"email":"{}","password":"{}"}}"#, e, p);
            let res = http_post(&h, "/api/auth/login", &body)?;
            assert_status(&res, 200)?;
            assert_json_has(&res, "token")?;
            Ok(())
        }, &mut passed, &mut failed);

        // Extract token for authenticated tests
        let body = format!(r#"{{"email":"{}","password":"{}"}}"#, email, password);
        if let Ok(res) = http_post(&host, "/api/auth/login", &body) {
            if let Some(t) = extract_json_value(&res, "token") {
                token = t;
            }
        }
    }

    // ── Collections API ──────────────────────────────────────────────────────

    test(&host, "GET /api/collections without auth returns 401", || {
        let res = http_get(&host, "/api/collections")?;
        assert_status(&res, 401)?;
        Ok(())
    }, &mut passed, &mut failed);

    if !token.is_empty() {
        let t = token.clone();
        test(&host, "GET /api/collections with auth returns 200", || {
            let res = http_get_auth(&host, "/api/collections", &t)?;
            assert_status(&res, 200)?;
            assert_json_has(&res, "collections")?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "GET /api/auth/me with token returns 200", || {
            let res = http_get_auth(&host, "/api/auth/me", &t)?;
            assert_status(&res, 200)?;
            assert_json_has(&res, "email")?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "POST /api/admin/chat lists collections and includes _users", || {
            let body = r#"{"model":"ministral-3:8b","messages":[{"role":"user","content":"List collections using the list_collections tool."}],"stream":false}"#;
            let res = http_post_auth_timeout(&host, "/api/admin/chat", body, &t, 30)?;
            assert_status(&res, 200)?;
            let content = extract_message_content(&res).ok_or("Missing message content")?;
            assert_contains(&content, "\\\"answer\\\"")?;
            assert_contains(&content, "\\\"data\\\"")?;
            assert_contains(&content, "\\\"collections\\\"")?;
            assert_contains(&content, "\\\"system_collections\\\"")?;
            assert_contains(&content, "_users")?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "POST /api/admin/chat lists system collections only", || {
            let body = r#"{"model":"ministral-3:8b","messages":[{"role":"user","content":"List system collections using the list_system_collections tool."}],"stream":false}"#;
            let res = http_post_auth_timeout(&host, "/api/admin/chat", body, &t, 30)?;
            assert_status(&res, 200)?;
            let content = extract_message_content(&res).ok_or("Missing message content")?;
            assert_contains(&content, "\\\"system_collections\\\"")?;
            if content.contains("\\\"collections\\\"") {
                return Err("System collections response should not include collections".into());
            }
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "POST /api/admin/chat lists project collections only", || {
            let body = r#"{"model":"ministral-3:8b","messages":[{"role":"user","content":"List project collections using the list_project_collections tool."}],"stream":false}"#;
            let res = http_post_auth_timeout(&host, "/api/admin/chat", body, &t, 30)?;
            assert_status(&res, 200)?;
            let content = extract_message_content(&res).ok_or("Missing message content")?;
            assert_contains(&content, "\\\"collections\\\"")?;
            if content.contains("\\\"system_collections\\\"") {
                return Err("Project collections response should not include system_collections".into());
            }
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "POST /api/admin/chat lists projects", || {
            let body = r#"{"model":"ministral-3:8b","messages":[{"role":"user","content":"List projects using the list_projects tool."}],"stream":false}"#;
            let res = http_post_auth_timeout(&host, "/api/admin/chat", body, &t, 30)?;
            assert_status(&res, 200)?;
            let content = extract_message_content(&res).ok_or("Missing message content")?;
            assert_contains(&content, "\\\"projects\\\"")?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "POST /api/projects creates project and template includes name", || {
            let name = "hc-test-project";
            let _ = http_delete_auth(&host, &format!("/api/projects/{}", name), &t);

            let body = format!(r#"{{"name":"{}"}}"#, name);
            let res = http_post_auth(&host, "/api/projects", &body, &t)?;
            if let Err(_) = assert_status(&res, 201) {
                return Err(format!("Create failed: {}", extract_body(&res)));
            }

            let page = http_get(&host, &format!("/projects/{}/", name))?;
            assert_status(&page, 200)?;
            assert_contains(&page, name)?;
            assert_contains(&page, &format!("Project: {}", name))?;
            assert_contains(&page, &format!("Project {}", name))?;
            assert_contains(&page, "admin@admin.com")?;

            let ports = http_get_auth(&host, "/api/collections/_ports", &t)?;
            assert_status(&ports, 200)?;
            assert_contains(&ports, name)?;
            assert_contains(&ports, "\"dev_port\"")?;
            assert_contains(&ports, "\"prod_port\"")?;

            let collections = http_get_auth(&host, "/api/collections", &t)?;
            assert_status(&collections, 200)?;
            assert_contains(&collections, &format!("dev-{}_users", name))?;
            assert_contains(&collections, &format!("dev-{}_sessions", name))?;
            assert_contains(&collections, &format!("dev-{}_settings", name))?;

            let del = http_delete_auth(&host, &format!("/api/projects/{}", name), &t)?;
            assert_status(&del, 200)?;

            let ports_after = http_get_auth(&host, "/api/collections/_ports", &t)?;
            assert_status(&ports_after, 200)?;
            if ports_after.contains(name) {
                return Err("Project ports still present after delete".into());
            }

            let page_after = http_get(&host, &format!("/projects/{}/", name))?;
            assert_status(&page_after, 404)?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "GET /api/admin/stats with admin returns 200", || {
            let res = http_get_auth(&host, "/api/admin/stats", &t)?;
            assert_status(&res, 200)?;
            assert_json_has(&res, "collections")?;
            assert_json_has(&res, "users")?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "PUT /api/admin/settings persists infra + port fields", || {
            let settings = http_get_auth(&host, "/api/admin/settings", &t)?;
            assert_status(&settings, 200)?;
            let settings_id = extract_json_value(&settings, "id").unwrap_or_default();
            if settings_id.is_empty() {
                return Err("Missing settings id".into());
            }

            let body = format!(
                r#"{{"id":"{}","nginx_hostname":"proxy-test.local","dev_network_subnet":"10.35.0.0/24","prod_network_subnet":"10.36.0.0/24","dev_ip_base":"10.35.0.","prod_ip_base":"10.36.0.","dev_port_start":3501,"prod_port_end":3699}}"#,
                settings_id
            );
            let res = http_put_auth(&host, "/api/admin/settings", &body, &t)?;
            assert_status(&res, 200)?;

            let settings_after = http_get_auth(&host, "/api/admin/settings", &t)?;
            assert_status(&settings_after, 200)?;
            assert_contains(&settings_after, "proxy-test.local")?;
            assert_contains(&settings_after, "10.35.0.0/24")?;
            assert_contains(&settings_after, "10.36.0.0/24")?;
            assert_contains(&settings_after, "10.35.0.")?;
            assert_contains(&settings_after, "10.36.0.")?;
            assert_contains(&settings_after, "\"dev_port_start\": 3501")?;
            assert_contains(&settings_after, "\"prod_port_end\": 3699")?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "GET /api/collections excludes system collections", || {
            let res = http_get_auth(&host, "/api/collections", &t)?;
            assert_status(&res, 200)?;
            if res.contains("_settings") || res.contains("_ports") {
                return Err("System collections should be hidden from /api/collections".into());
            }
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "GET /api/admin/collections/system includes _settings and _ports", || {
            let res = http_get_auth(&host, "/api/admin/collections/system", &t)?;
            assert_status(&res, 200)?;
            assert_contains(&res, "_settings")?;
            assert_contains(&res, "_ports")?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "POST /api/admin/chat find_free_ports returns dev/prod", || {
            let body = r#"{"model":"ministral-3:8b","messages":[{"role":"user","content":"Find free ports using the find_free_ports tool."}],"stream":false}"#;
            let res = http_post_auth_timeout(&host, "/api/admin/chat", body, &t, 30)?;
            assert_status(&res, 200)?;
            let content = extract_message_content(&res).ok_or("Missing message content")?;
            assert_contains(&content, "\\\"dev_port\\\"")?;
            assert_contains(&content, "\\\"prod_port\\\"")?;
            Ok(())
        }, &mut passed, &mut failed);

        let t = token.clone();
        test(&host, "Project port assignments are unique and offset", || {
            let name_a = "hc-port-a";
            let name_b = "hc-port-b";
            let _ = http_delete_auth(&host, &format!("/api/projects/{}", name_a), &t);
            let _ = http_delete_auth(&host, &format!("/api/projects/{}", name_b), &t);

            let res_a = http_post_auth(&host, "/api/projects", &format!(r#"{{"name":"{}"}}"#, name_a), &t)?;
            if let Err(_) = assert_status(&res_a, 201) {
                return Err(format!("Create A failed: {}", extract_body(&res_a)));
            }
            let res_b = http_post_auth(&host, "/api/projects", &format!(r#"{{"name":"{}"}}"#, name_b), &t)?;
            if let Err(_) = assert_status(&res_b, 201) {
                return Err(format!("Create B failed: {}", extract_body(&res_b)));
            }

            let ports = http_get_auth(&host, "/api/collections/_ports", &t)?;
            assert_status(&ports, 200)?;
            let (a_dev, a_prod) = extract_ports_for_project(&ports, name_a).ok_or("Missing ports for hc-port-a")?;
            let (b_dev, b_prod) = extract_ports_for_project(&ports, name_b).ok_or("Missing ports for hc-port-b")?;
            if a_dev == b_dev || a_prod == b_prod {
                return Err("Port assignments should be unique".into());
            }
            if a_prod as i64 - a_dev as i64 != 100 {
                return Err("Prod port should be dev+100".into());
            }
            if b_prod as i64 - b_dev as i64 != 100 {
                return Err("Prod port should be dev+100".into());
            }

            let _ = http_delete_auth(&host, &format!("/api/projects/{}", name_a), &t);
            let _ = http_delete_auth(&host, &format!("/api/projects/{}", name_b), &t);
            Ok(())
        }, &mut passed, &mut failed);

        // ── E-commerce Batch Test ────────────────────────────────────────────
        println!("\n  📦 E-commerce batch test:");

        let ecommerce_result = run_ecommerce_batch_test(&host, &token);
        match ecommerce_result {
            Ok(()) => {
                println!("  ✓ E-commerce batch: create, verify, cleanup");
                passed += 1;
            }
            Err(e) => {
                println!("  ✗ E-commerce batch test failed: {}", e);
                failed += 1;
            }
        }
    }

    // ── Dev endpoints ────────────────────────────────────────────────────────

    test(&host, "GET /__dev/mtime returns timestamp", || {
        let res = http_get(&host, "/__dev/mtime")?;
        assert_status(&res, 200)?;
        let body = get_body(&res);
        if body.parse::<u64>().is_err() {
            return Err("Expected numeric mtime".into());
        }
        Ok(())
    }, &mut passed, &mut failed);

    // ── Summary ──────────────────────────────────────────────────────────────

    println!("\n{}", "─".repeat(50));
    if failed == 0 {
        println!("✅ All {} tests passed!", passed);
        std::process::exit(0);
    } else {
        println!("❌ {} passed, {} failed", passed, failed);
        std::process::exit(1);
    }
}

// ── Test Runner ──────────────────────────────────────────────────────────────

fn test<F>(_host: &str, name: &str, f: F, passed: &mut u32, failed: &mut u32)
where
    F: FnOnce() -> Result<(), String>,
{
    match f() {
        Ok(()) => {
            println!("  ✓ {}", name);
            *passed += 1;
        }
        Err(e) => {
            println!("  ✗ {} - {}", name, e);
            *failed += 1;
        }
    }
}

// ── HTTP Client ──────────────────────────────────────────────────────────────

fn http_get(host: &str, path: &str) -> Result<String, String> {
    http_request(host, "GET", path, None, None)
}

fn http_get_auth(host: &str, path: &str, token: &str) -> Result<String, String> {
    http_request(host, "GET", path, None, Some(token))
}

fn http_post(host: &str, path: &str, body: &str) -> Result<String, String> {
    http_request(host, "POST", path, Some(body), None)
}

fn http_post_auth(host: &str, path: &str, body: &str, token: &str) -> Result<String, String> {
    http_request(host, "POST", path, Some(body), Some(token))
}

fn http_put_auth(host: &str, path: &str, body: &str, token: &str) -> Result<String, String> {
    http_request(host, "PUT", path, Some(body), Some(token))
}

fn http_delete_auth(host: &str, path: &str, token: &str) -> Result<String, String> {
    http_request(host, "DELETE", path, None, Some(token))
}

fn http_post_auth_timeout(
    host: &str,
    path: &str,
    body: &str,
    token: &str,
    timeout_secs: u64,
) -> Result<String, String> {
    http_request_timeout(host, "POST", path, Some(body), Some(token), timeout_secs)
}

fn http_request(host: &str, method: &str, path: &str, body: Option<&str>, token: Option<&str>) -> Result<String, String> {
    http_request_timeout(host, method, path, body, token, 5)
}

fn http_request_timeout(
    host: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    token: Option<&str>,
    timeout_secs: u64,
) -> Result<String, String> {
    let (host_name, port) = split_host_port(host);
    let mut stream = connect_stream(&host_name, port, timeout_secs)?;

    let body_bytes = body.unwrap_or("");
    let mut request = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        method, path, host_name, body_bytes.len()
    );

    if let Some(t) = token {
        request.push_str(&format!("Authorization: Bearer {}\r\n", t));
    }

    request.push_str("\r\n");
    request.push_str(body_bytes);

    stream.write_all(request.as_bytes())
        .map_err(|e| format!("Write failed: {}", e))?;

    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("Read failed: {}", e)),
        }
    }

    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn connect_stream(host: &str, port: u16, timeout_secs: u64) -> Result<Box<dyn ReadWrite>, String> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(addr)
        .map_err(|e| format!("Connection failed: {}", e))?;
    stream.set_read_timeout(Some(Duration::from_secs(timeout_secs))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();

    if port == 443 {
        let cfg = insecure_client_config();
        let server_name = ServerName::try_from(host.to_string())
            .map_err(|_| "Invalid server name".to_string())?;
        let tls = rustls::ClientConnection::new(Arc::new(cfg), server_name)
            .map_err(|e| format!("TLS init failed: {}", e))?;
        let tls_stream = rustls::StreamOwned::new(tls, stream);
        Ok(Box::new(tls_stream))
    } else {
        Ok(Box::new(stream))
    }
}

fn split_host_port(host: &str) -> (String, u16) {
    if let Some((h, p)) = host.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            return (h.to_string(), port);
        }
    }
    (host.to_string(), 80)
}

fn insecure_client_config() -> ClientConfig {
    let verifier = Arc::new(InsecureVerifier);
    let mut cfg = ClientConfig::builder()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    cfg.dangerous().set_certificate_verifier(verifier);
    cfg
}

#[derive(Debug)]
struct InsecureVerifier;

impl ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
        ]
    }
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

// ── Assertions ───────────────────────────────────────────────────────────────

fn assert_status(response: &str, expected: u16) -> Result<(), String> {
    let status_line = response.lines().next().unwrap_or("");
    let expected_str = format!("{}", expected);
    if status_line.contains(&expected_str) {
        Ok(())
    } else {
        Err(format!("Expected status {}, got: {}", expected, status_line))
    }
}

fn assert_contains(response: &str, needle: &str) -> Result<(), String> {
    if response.contains(needle) {
        Ok(())
    } else {
        Err(format!("Response missing: {}", needle))
    }
}

fn extract_body(response: &str) -> String {
    response.split("\r\n\r\n").nth(1).unwrap_or("").trim().to_string()
}

fn assert_json_has(response: &str, key: &str) -> Result<(), String> {
    let body = get_body(response);
    let pattern = format!(r#""{}""#, key);
    if body.contains(&pattern) {
        Ok(())
    } else {
        Err(format!("JSON missing key: {}", key))
    }
}

fn get_body(response: &str) -> &str {
    response.split("\r\n\r\n").nth(1).unwrap_or("")
}

fn extract_message_content(response: &str) -> Option<String> {
    let body = get_body(response);
    let key = "\"content\":\"";
    let start = body.find(key)? + key.len();
    let mut escaped = false;
    for (i, ch) in body[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return Some(body[start..start + i].to_string());
        }
    }
    None
}

fn extract_ports_for_project(body: &str, project: &str) -> Option<(i64, i64)> {
    let mut dev = None;
    let mut prod = None;
    let mut in_obj = false;
    let mut obj = String::new();
    for ch in body.chars() {
        if ch == '{' {
            in_obj = true;
            obj.clear();
        }
        if in_obj {
            obj.push(ch);
        }
        if ch == '}' && in_obj {
            in_obj = false;
            if obj.contains(&format!(r#""project":"{}""#, project))
                || obj.contains(&format!(r#""project": "{}""#, project))
            {
                dev = extract_number(&obj, "dev_port");
                prod = extract_number(&obj, "prod_port");
                break;
            }
        }
    }
    match (dev, prod) {
        (Some(d), Some(p)) => Some((d, p)),
        _ => None,
    }
}

fn extract_number(body: &str, key: &str) -> Option<i64> {
    let pattern = format!(r#""{}":"#, key);
    let start = body.find(&pattern)? + pattern.len();
    let rest = &body[start..];
    let rest = rest.trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse::<i64>().ok()
}

fn extract_json_value(response: &str, key: &str) -> Option<String> {
    let body = get_body(response);
    let pattern = format!(r#""{}":""#, key);
    let start = body.find(&pattern)? + pattern.len();
    let rest = &body[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// ── Env Loader ───────────────────────────────────────────────────────────────

fn load_env() -> HashMap<String, String> {
    let mut env = HashMap::new();

    // Try .env.local first, then .env
    for path in &[".env.local", ".env"] {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let value = value.trim().trim_matches('"').trim_matches('\'');
                    env.insert(key.trim().to_string(), value.to_string());
                }
            }
        }
    }

    // Override with actual env vars
    for key in ["ADMIN_EMAIL", "ADMIN_PASSWORD"] {
        if let Ok(val) = std::env::var(key) {
            env.insert(key.to_string(), val);
        }
    }

    env
}

// ── E-commerce Batch Test ────────────────────────────────────────────────────

fn run_ecommerce_batch_test(host: &str, token: &str) -> Result<(), String> {
    // Collection definitions: (name, fields_json, test_doc)
    let collections: [(&str, &str, &str); 5] = [
        ("test_categories",
         r#"[{"name":"name","type":"string"},{"name":"description","type":"string"}]"#,
         r#"{"name":"Electronics","description":"Electronic devices and gadgets"}"#),
        ("test_products",
         r#"[{"name":"title","type":"string"},{"name":"price","type":"int"},{"name":"category","type":"string"},{"name":"stock","type":"int"}]"#,
         r#"{"title":"Laptop","price":999,"category":"Electronics","stock":50}"#),
        ("test_customers",
         r#"[{"name":"fullname","type":"string"},{"name":"email","type":"string"},{"name":"address","type":"string"}]"#,
         r#"{"fullname":"John Doe","email":"john@example.com","address":"123 Main St"}"#),
        ("test_orders",
         r#"[{"name":"customer_id","type":"string"},{"name":"total","type":"int"},{"name":"status","type":"string"}]"#,
         r#"{"customer_id":"cust_123","total":1999,"status":"pending"}"#),
        ("test_reviews",
         r#"[{"name":"product_id","type":"string"},{"name":"rating","type":"int"},{"name":"comment","type":"string"}]"#,
         r#"{"product_id":"prod_123","rating":5,"comment":"Great product!"}"#),
    ];

    let mut created_collections: Vec<String> = Vec::new();
    let mut created_docs: Vec<(String, String)> = Vec::new();

    // Phase 1: Create collections
    print!("     Creating collections... ");
    for (name, fields, _) in &collections {
        let schema = format!(r#"{{"name":"{}","fields":{}}}"#, name, fields);
        let res = http_post_auth(host, "/api/collections", &schema, token)?;
        if !res.contains("201") {
            cleanup_ecommerce(host, token, &created_collections, &created_docs);
            return Err(format!("Failed to create collection {}: {}", name, get_body(&res)));
        }
        created_collections.push(name.to_string());
    }
    println!("✓");

    // Phase 2: Insert test documents
    print!("     Inserting test data... ");
    for (name, _, test_data) in &collections {
        let path = format!("/api/collections/{}", name);
        let res = http_post_auth(host, &path, test_data, token)?;
        if !res.contains("201") {
            cleanup_ecommerce(host, token, &created_collections, &created_docs);
            return Err(format!("Failed to insert doc into {}: {}", name, get_body(&res)));
        }
        if let Some(id) = extract_json_value(&res, "id") {
            created_docs.push((name.to_string(), id));
        }
    }
    println!("✓");

    // Phase 3: Verify documents exist
    print!("     Verifying data... ");
    for (collection, doc_id) in &created_docs {
        let path = format!("/api/collections/{}/{}", collection, doc_id);
        let res = http_get_auth(host, &path, token)?;
        if !res.contains("200") {
            cleanup_ecommerce(host, token, &created_collections, &created_docs);
            return Err(format!("Document {} not found in {}", doc_id, collection));
        }
    }
    println!("✓");

    // Phase 4: Verify collection listing includes our test collections
    print!("     Verifying collections list... ");
    let res = http_get_auth(host, "/api/collections", token)?;
    for name in &created_collections {
        if !res.contains(name) {
            cleanup_ecommerce(host, token, &created_collections, &created_docs);
            return Err(format!("Collection {} not in list", name));
        }
    }
    println!("✓");

    // Phase 5: Cleanup - delete documents first, then collections
    print!("     Cleaning up... ");
    cleanup_ecommerce(host, token, &created_collections, &created_docs);
    println!("✓");

    // Phase 6: Verify cleanup
    print!("     Verifying cleanup... ");
    let res = http_get_auth(host, "/api/collections", token)?;
    for name in &created_collections {
        if res.contains(name) {
            return Err(format!("Collection {} still exists after cleanup", name));
        }
    }
    println!("✓");

    Ok(())
}

fn cleanup_ecommerce(host: &str, token: &str, collections: &[String], docs: &[(String, String)]) {
    // Delete documents first
    for (collection, doc_id) in docs {
        let path = format!("/api/collections/{}/{}", collection, doc_id);
        let _ = http_delete_auth(host, &path, token);
    }

    // Delete collections
    for name in collections {
        let path = format!("/api/collections/{}", name);
        let _ = http_delete_auth(host, &path, token);
    }
}
