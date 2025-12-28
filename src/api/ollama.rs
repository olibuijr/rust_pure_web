//! Ollama API proxy
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use crate::api::{Request, Response};
use crate::api::utils::{require_admin, load_env};
use crate::api::tools;
use crate::api::json::JsonSerializer as Json;

pub fn chat(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    
    let ollama_url = load_env("OLLAMA_HOST").unwrap_or_else(|| "http://localhost:11434".to_string());
    let host_port = ollama_url.trim_start_matches("http://").trim_start_matches("https://");
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        (h, p)
    } else {
        (host_port, "11434")
    };

    // Prepare the initial request to Ollama with tools
    let mut ollama_req_body = req.body.clone();
    ollama_req_body = ensure_system_prompt(&ollama_req_body);
    
    // Inject tools if not already present
    if !ollama_req_body.contains("\"tools\":") {
        if let Some(pos) = ollama_req_body.rfind('}') {
            let tools_json = tools::get_tools_json();
            // Manually splice JSON safely
            ollama_req_body = format!(
                "{},\"tools\":{}}}",
                &ollama_req_body[..pos],
                tools_json
            );
        }
    }

    match forward_to_ollama(host, port, &ollama_req_body) {
        Ok(res_body) => {
            if res_body.contains("\"tool_calls\":") {
                handle_tool_calls(host, port, &ollama_req_body, &res_body)
            } else {
                Response::ok(&res_body)
            }
        },
        Err(e) => Response::bad_request(&format!("Ollama error: {}", e)),
    }
}

fn ensure_system_prompt(body: &str) -> String {
    if body.contains(r#""role":"system""#) {
        return body.to_string();
    }
    let msg_key = r#""messages":"#;
    let msg_pos = match body.find(msg_key) {
        Some(pos) => pos,
        None => return body.to_string(),
    };
    let bracket_pos = match body[msg_pos..].find('[') {
        Some(pos) => msg_pos + pos + 1,
        None => return body.to_string(),
    };
    let system_msg = r#"{"role":"system","content":"You have tool access: list_collections (returns project collections plus system collections), list_project_collections (project collections only), list_system_collections (internal collections starting with _), list_projects, create_project, find_free_ports, and search_docs. Use tools when asked about collections, projects, ports, or docs. System collections include _users, _settings, and _sessions. Always respond with strict JSON only (no markdown, no extra text). Use shape {\"answer\":string,\"data\":object,\"error\":string|null}."},"#;
    let mut out = String::with_capacity(body.len() + system_msg.len());
    out.push_str(&body[..bracket_pos]);
    out.push_str(system_msg);
    out.push_str(&body[bracket_pos..]);
    out
}

fn handle_tool_calls(_host: &str, _port: &str, _original_req: &str, ollama_res: &str) -> Response {
    if let Some(tc_start) = ollama_res.find("\"tool_calls\":") {
        if let Some(name_start) = ollama_res[tc_start..].find("\"name\":\"") {
            let name_pos = tc_start + name_start + 8;
            if let Some(name_end) = ollama_res[name_pos..].find('"') {
                let tool_name = &ollama_res[name_pos..name_pos + name_end];
                
                let mut args = "";
                if let Some(args_start) = ollama_res[name_pos..].find("\"arguments\":") {
                    let args_pos = name_pos + args_start + 12;
                    if let Some(arg_val_start) = ollama_res[args_pos..].find('{') {
                        let mut brace_count = 0;
                        let mut arg_val_end = 0;
                        for (i, c) in ollama_res[args_pos + arg_val_start..].chars().enumerate() {
                            if c == '{' { brace_count += 1; }
                            if c == '}' { brace_count -= 1; }
                            if brace_count == 0 {
                                arg_val_end = i + 1;
                                break;
                            }
                        }
                        args = &ollama_res[args_pos + arg_val_start .. args_pos + arg_val_start + arg_val_end];
                    }
                }

                let tool_result = tools::call_tool(tool_name, args);
                let answer = match tool_name {
                    "list_collections" => "Here are your collections.",
                    "list_project_collections" => "Here are your project collections.",
                    "list_system_collections" => "Here are your system collections.",
                    "list_projects" => "Here are your projects.",
                    "create_project" => "Project created.",
                    "find_free_ports" => "Here is a free dev/prod port pair.",
                    "search_docs" => "Here are the matching docs.",
                    _ => "Here is the result.",
                };
                let msg_content = format!(
                    "{{\"answer\":{},\"data\":{},\"error\":null}}",
                    Json::wrap_string(answer),
                    tool_result
                );
                let inner_msg = format!("{{{},{}}}",
                    Json::key_string("role", "assistant"),
                    Json::key_string("content", &msg_content)
                );
                let response = format!("{{\"message\":{}}}", inner_msg);
                
                Response::ok(&response)
            } else { Response::ok(ollama_res) }
        } else { Response::ok(ollama_res) }
    } else { Response::ok(ollama_res) }
}

fn forward_to_ollama(host: &str, port: &str, body: &str) -> Result<String, String> {
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("Failed to connect to Ollama at {}: {}", addr, e))?;

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();

    let request = format!(
        "POST /api/chat HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        host, body.len(), body
    );

    stream.write_all(request.as_bytes())
        .map_err(|e| format!("Failed to write to Ollama: {}", e))?;

    let mut response = String::new();
    stream.read_to_string(&mut response)
        .map_err(|e| format!("Failed to read from Ollama: {}", e))?;

    if let Some(body_start) = response.find("\r\n\r\n") {
        Ok(response[body_start + 4..].to_string())
    } else {
        Ok(response)
    }
}
