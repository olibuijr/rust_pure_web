//! API routing and JSON handling
pub mod admin;
pub mod auth;
pub mod collections;
pub mod contact;
pub mod json;
pub mod ollama;
pub mod projects;
pub mod tools;
pub mod utils;

// Note: admin is now a single file instead of a subdirectory

use std::collections::HashMap;
use crate::logging;

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn json(status: u16, data: &str) -> Self {
        Self { status, body: data.to_string() }
    }
    pub fn ok(data: &str) -> Self { Self::json(200, data) }
    pub fn created(data: &str) -> Self { Self::json(201, data) }
    pub fn bad_request(msg: &str) -> Self { Self::json(400, &format!(r#"{{"error":"{}"}}"#, msg)) }
    pub fn unauthorized() -> Self { Self::json(401, r#"{"error":"Unauthorized"}"#) }
    pub fn not_found() -> Self { Self::json(404, r#"{"error":"Not found"}"#) }
}

/// Route API request
pub fn handle(req: &Request) -> Response {
    let path_parts: Vec<&str> = req.path.trim_start_matches("/api/").split('/').collect();

    let response = match (req.method.as_str(), path_parts.as_slice()) {
        // Auth routes
        ("POST", ["auth", "register"]) => auth::register(req),
        ("POST", ["auth", "login"]) => auth::login(req),
        ("POST", ["auth", "logout"]) => auth::logout(req),
        ("GET", ["auth", "me"]) => auth::me(req),

        // Collection routes
        ("GET", ["collections"]) => collections::list_collections(req),
        ("POST", ["collections"]) => collections::create_collection(req),
        ("DELETE", ["collections", name]) => collections::delete_collection(req, name),
        ("GET", ["collections", name]) => collections::list_documents(req, name),
        ("POST", ["collections", name]) => collections::create_document(req, name),
        ("GET", ["collections", name, id]) => collections::get_document(req, name, id),
        ("PUT", ["collections", name, id]) => collections::update_document(req, name, id, &req.body),
        ("DELETE", ["collections", name, id]) => collections::delete_document(req, name, id),

        // Admin routes
        ("GET", ["admin", "stats"]) => admin::stats(req),
        ("POST", ["admin", "backup"]) => admin::backup(req),
        ("GET", ["admin", "collections", "system"]) => collections::list_system_collections(req),
        ("GET", ["admin", "users"]) => admin::list_users(req),
        ("POST", ["admin", "users"]) => admin::create_user(req),
        ("PUT", ["admin", "users", id]) => admin::update_user(req, id),
        ("DELETE", ["admin", "users", id]) => admin::delete_user(req, id),
        ("GET", ["admin", "settings"]) => admin::get_settings(req),
        ("PUT", ["admin", "settings"]) => admin::update_settings(req),
        ("POST", ["admin", "chat"]) => ollama::chat(req),
        ("POST", ["contact"]) => contact::submit(req),

        // Projects routes
        ("GET", ["projects"]) => projects::list_projects(req),
        ("POST", ["projects"]) => projects::create_project(req),
        ("DELETE", ["projects", name]) => projects::delete_project(req, name),

        _ => Response::not_found(),
    };
    logging::info("api", &format!("{} {} -> {}", req.method, req.path, response.status));
    response
}
