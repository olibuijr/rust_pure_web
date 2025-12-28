//! AI Agent Tools execution
use std::fs;
use crate::db;
use crate::config;
use crate::api::json::JsonSerializer as Json;
use crate::api::json::parse_json;
use crate::ports;
use crate::db::Value;

/// Define available tools for Ollama
pub fn get_tools_json() -> String {
    r#"[
        {
            "type": "function",
            "function": {
                "name": "list_collections",
                "description": "List project collections and internal system collections in one response",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_project_collections",
                "description": "List project collections only",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_system_collections",
                "description": "List internal system collections (names starting with _)",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_projects",
                "description": "List project folders",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "create_project",
                "description": "Create a new project from the template",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Project folder name"
                        }
                    },
                    "required": ["name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "find_free_ports",
                "description": "Find a free dev/prod port pair based on configured ranges",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search_docs",
                "description": "Search the internal documentation for a specific query",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search term or topic to look up"
                        }
                    },
                    "required": ["query"]
                }
            }
        }
    ]"#.to_string()
}

/// Execute a tool call
pub fn call_tool(name: &str, args: &str) -> String {
    match name {
        "list_collections" => list_collections(),
        "list_project_collections" => list_project_collections(),
        "list_system_collections" => list_system_collections(),
        "list_projects" => list_projects(),
        "create_project" => create_project(args),
        "find_free_ports" => find_free_ports(),
        "search_docs" => search_docs(args),
        _ => format!("{{\"error\":\"Unknown tool: {}\"}}", name),
    }
}

fn list_collections() -> String {
    let db = db::get();
    let collections = db.list_collections();
    let list = collections
        .iter()
        .map(|name| Json::wrap_string(name))
        .collect::<Vec<_>>()
        .join(",");
    let system = db
        .list_all_collections()
        .iter()
        .filter(|name| name.starts_with('_'))
        .map(|name| Json::wrap_string(name))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"collections\":[{}],\"system_collections\":[{}]}}", list, system)
}

fn list_project_collections() -> String {
    let db = db::get();
    let collections = db.list_collections();
    let list = collections
        .iter()
        .map(|name| Json::wrap_string(name))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"collections\":[{}]}}", list)
}

fn list_system_collections() -> String {
    let db = db::get();
    let collections = db.list_all_collections();
    let list = collections
        .iter()
        .filter(|name| name.starts_with('_'))
        .map(|name| Json::wrap_string(name))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"system_collections\":[{}]}}", list)
}

fn list_projects() -> String {
    let projects_dir = config::root_dir().join("projects");
    if !projects_dir.exists() {
        return "{\"projects\":[]}".to_string();
    }
    let mut projects = Vec::new();
    if let Ok(entries) = fs::read_dir(projects_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name != "_template" {
                        projects.push(name.to_string());
                    }
                }
            }
        }
    }
    projects.sort();
    let list = projects
        .iter()
        .map(|name| Json::wrap_string(name))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"projects\":[{}]}}", list)
}

fn create_project(args_json: &str) -> String {
    let json = parse_json(args_json);
    let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("");

    if name.is_empty() || name.starts_with('_') || name.contains('/') || name.contains('.') {
        return "{\"error\":\"Invalid project name\"}".to_string();
    }

    let root = config::root_dir();
    let projects_dir = root.join("projects");
    let template_dir = projects_dir.join("_template");
    let target_dir = projects_dir.join(name);

    if target_dir.exists() {
        return "{\"error\":\"Project already exists\"}".to_string();
    }
    if !template_dir.exists() {
        return "{\"error\":\"Template not found\"}".to_string();
    }
    if let Err(e) = copy_dir(&template_dir, &target_dir) {
        return format!("{{\"error\":\"Failed to clone template: {}\"}}", e);
    }

    format!("{{\"name\":{},\"success\":true}}", Json::wrap_string(name))
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn find_free_ports() -> String {
    let settings = db::get().find_all("_settings");
    let (dev_start, dev_end, prod_start, prod_end) = if let Some(doc) = settings.first() {
        (
            match doc.get("dev_port_start") { Some(Value::Int(v)) => *v as u16, _ => 3501 },
            match doc.get("dev_port_end") { Some(Value::Int(v)) => *v as u16, _ => 3599 },
            match doc.get("prod_port_start") { Some(Value::Int(v)) => *v as u16, _ => 3601 },
            match doc.get("prod_port_end") { Some(Value::Int(v)) => *v as u16, _ => 3699 },
        )
    } else {
        (3501, 3599, 3601, 3699)
    };

    match ports::find_free_port_pair(dev_start, dev_end, prod_start, prod_end) {
        Some((dev, prod)) => format!("{{\"dev_port\":{},\"prod_port\":{}}}", dev, prod),
        None => "{\"error\":\"No free ports available\"}".to_string(),
    }
}

fn search_docs(args_json: &str) -> String {
    // Basic JSON extraction without dependencies
    let query = if let Some(start) = args_json.find("\"query\":") {
        let rest = &args_json[start + 8..];
        let rest = rest.trim_start_matches(':').trim_start_matches(' ').trim_start_matches('"');
        if let Some(end) = rest.find('"') {
            &rest[..end]
        } else { "" }
    } else { "" };

    if query.is_empty() {
        return "{{\"error\":\"Missing query parameter\"}}".to_string();
    }

    let docs_dir = config::templates_dir().join("docs");
    let mut results = Vec::new();

    if let Ok(entries) = fs::read_dir(docs_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if content.to_lowercase().contains(&query.to_lowercase()) {
                    if let Some(name) = entry.file_name().to_str() {
                        let preview = content
                            .chars()
                            .take(100)
                            .collect::<String>()
                            .replace('"', "'")
                            .replace('\n', " ")
                            .replace('\r', " ");
                        let item = format!(
                            "{{{},{}}}",
                            Json::key_string("file", name),
                            Json::key_string("preview", &preview)
                        );
                        results.push(item);
                    }
                }
            }
        }
    }

    format!("{{\"results\":[{}]}}", results.join(","))
}
