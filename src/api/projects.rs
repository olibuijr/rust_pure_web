//! Projects API handlers
use std::fs;
use crate::config;
use crate::api::{Request, Response};
use crate::api::json::parse_json;
use crate::api::utils::require_admin;
use crate::ports;
use crate::crypto::hash_password;
use crate::db::{self, Document, Value};

pub fn list_projects(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    
    let projects_dir = config::root_dir().join("projects");
    if !projects_dir.exists() {
        return Response::ok("[]");
    }

    let mut projects = Vec::new();
    if let Ok(entries) = fs::read_dir(projects_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name != "_template" {
                        projects.push(format!("\"{}\"", name));
                    }
                }
            }
        }
    }
    projects.sort();
    Response::ok(&format!("[{}]", projects.join(",")))
}

pub fn create_project(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    
    let json = parse_json(&req.body);
    let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("");
    
    if name.is_empty() || name.starts_with('_') || name.contains('/') || name.contains('.') {
        return Response::bad_request("Invalid project name");
    }

    let root = config::root_dir();
    let projects_dir = root.join("projects");
    let template_dir = projects_dir.join("_template");
    let target_dir = projects_dir.join(name);

    if target_dir.exists() {
        return Response::bad_request("Project already exists");
    }

    if !template_dir.exists() {
        return Response::bad_request("Template not found");
    }

    // Assign dev/prod ports
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

    let (dev_port, prod_port) = match ports::find_free_port_pair(dev_start, dev_end, prod_start, prod_end) {
        Some(pair) => pair,
        None => return Response::bad_request("No free ports available"),
    };

    if let Err(e) = copy_dir(&template_dir, &target_dir) {
        return Response::bad_request(&format!("Failed to clone template: {}", e));
    }

    ensure_default_dev_user();
    ensure_project_collections(name);

    let mut port_doc = Document::new();
    port_doc.insert("project".into(), Value::String(name.to_string()));
    port_doc.insert("dev_port".into(), Value::Int(dev_port as i64));
    port_doc.insert("prod_port".into(), Value::Int(prod_port as i64));
    port_doc.insert("created".into(), Value::Int(db::now()));
    let _ = db::get().insert("_ports", port_doc);

    let index_path = target_dir.join("index.html");
    if let Ok(mut content) = fs::read_to_string(&index_path) {
        let project_nav = format!(
            r#"<nav class="fixed top-0 w-full border-b border-border bg-background/80 backdrop-blur-sm z-50">
        <div class="max-w-5xl mx-auto px-6 h-16 flex items-center justify-between">
            <a href="/projects/{name}/" class="font-semibold flex items-center gap-2">
                <span class="text-xl">🦀</span>
                <span>{name}</span>
            </a>
            <div class="flex items-center gap-4 text-sm text-muted-foreground">
                <span>Project: {name}</span>
                <a href="/docs" class="hover:text-foreground transition-colors font-medium text-blue-400">Docs</a>
                <a href="/" class="hover:text-foreground transition-colors">Home</a>
            </div>
        </div>
    </nav>"#,
            name = name
        );

        let project_footer = format!(
            r#"<footer class="py-8 px-6 border-t border-border">
        <div class="max-w-5xl mx-auto flex flex-col md:flex-row items-center justify-between gap-4 text-sm text-muted-foreground">
            <p>© 2024 Project {name}. All rights reserved.</p>
            <div class="flex items-center gap-4">
                <p>Built with Rust 🦀</p>
                <a href="/_admin" class="hover:text-foreground transition-colors">Admin</a>
            </div>
        </div>
    </footer>"#,
            name = name
        );

        content = content.replace("{% include \"components/nav.html\" %}", &project_nav);
        content = content.replace("{% include \"components/footer.html\" %}", &project_footer);
        content = content.replace("Hello World", name);
        let _ = fs::write(&index_path, content);
    }

    Response::created(&format!(r#"{{"name":"{}","success":true}}"#, name))
}

pub fn delete_project(req: &Request, name: &str) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    
    if name.starts_with('_') || name.contains('/') || name.contains('.') {
        return Response::bad_request("Invalid project name");
    }

    let target_dir = config::root_dir().join("projects").join(name);
    if !target_dir.exists() || !target_dir.is_dir() {
        return Response::not_found();
    }

    if let Err(e) = fs::remove_dir_all(target_dir) {
        return Response::bad_request(&format!("Failed to delete project: {}", e));
    }

    // Cleanup ports assignment
    cleanup_project_ports(name);
    cleanup_project_collections(name);

    Response::ok(r#"{"deleted":true}"#)
}

fn cleanup_project_ports(project: &str) {
    let docs = db::get().find_all("_ports");
    for doc in docs {
        let should_delete = doc
            .get("project")
            .and_then(Value::as_str)
            .map(|p| p == project)
            .unwrap_or(false);
        if !should_delete {
            continue;
        }
        if let Some(Value::String(id)) = doc.get("id") {
            let _ = db::get().delete("_ports", id);
        }
    }
}

fn ensure_default_dev_user() {
    let db = db::get();
    if db.find_by("_users", "email", "admin@admin.com").is_some() {
        return;
    }
    let mut doc = Document::new();
    doc.insert("email".into(), Value::String("admin@admin.com".into()));
    doc.insert("password".into(), Value::String(hash_password("password")));
    doc.insert("role".into(), Value::String("admin".into()));
    doc.insert("created".into(), Value::Int(db::now()));
    let _ = db.insert("_users", doc);
}

fn ensure_project_collections(project: &str) {
    let users = format!("dev-{}_users", project);
    let sessions = format!("dev-{}_sessions", project);
    let settings = format!("dev-{}_settings", project);

    if db::get().find_all(&users).is_empty() {
        db::get().create_collection(&users, vec![
            ("email".into(), "string".into()),
            ("password".into(), "string".into()),
            ("role".into(), "string".into()),
            ("created".into(), "int".into()),
        ]);
    }
    if db::get().find_all(&sessions).is_empty() {
        db::get().create_collection(&sessions, vec![
            ("user_id".into(), "string".into()),
            ("token".into(), "string".into()),
            ("expires".into(), "int".into()),
        ]);
    }
    if db::get().find_all(&settings).is_empty() {
        db::get().create_collection(&settings, vec![
            ("project_name".into(), "string".into()),
            ("created".into(), "int".into()),
        ]);
        let mut doc = Document::new();
        doc.insert("project_name".into(), Value::String(project.to_string()));
        doc.insert("created".into(), Value::Int(db::now()));
        let _ = db::get().insert(&settings, doc);
    }
}

fn cleanup_project_collections(project: &str) {
    let users = format!("dev-{}_users", project);
    let sessions = format!("dev-{}_sessions", project);
    let settings = format!("dev-{}_settings", project);
    let _ = db::get().delete_collection(&users);
    let _ = db::get().delete_collection(&sessions);
    let _ = db::get().delete_collection(&settings);
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
