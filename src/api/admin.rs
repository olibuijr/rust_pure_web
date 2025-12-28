//! Admin API handlers (stats, users, settings)
use crate::{auth, crypto, db};
use crate::api::{Request, Response};
use crate::api::json::parse_json;
use crate::api::utils::{get_token, require_admin, valid_email, valid_password, valid_role};
use crate::db::{Document, Value};

// ── Stats ────────────────────────────────────────────────────────────────────

pub fn stats(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let db = db::get();
    let collections = db.list_collections();
    let user_count = db.find_all("_users").len();
    Response::ok(&format!(
        r#"{{"collections":{},"users":{}}}"#,
        collections.len(), user_count
    ))
}

pub fn backup(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let path = db::get().backup();
    Response::ok(&format!(r#"{{"backup":"{}"}}"#, path))
}

// ── Users ────────────────────────────────────────────────────────────────────

pub fn list_users(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let users = db::get().find_all("_users");
    let json: Vec<String> = users.iter().map(|u| db::doc_to_json_for_collection("_users", u)).collect();
    Response::ok(&format!("[{}]", json.join(",")))
}

pub fn create_user(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let json = parse_json(&req.body);
    let email = json.get("email").and_then(|v| v.as_str()).unwrap_or("");
    let password = json.get("password").and_then(|v| v.as_str()).unwrap_or("");
    let role = json.get("role").and_then(|v| v.as_str()).unwrap_or("user");

    if !valid_email(email) {
        return Response::bad_request("Invalid email");
    }
    if !valid_password(password) {
        return Response::bad_request("Password must be at least 8 characters");
    }
    if !valid_role(role) {
        return Response::bad_request("Invalid role");
    }
    if db::get().find_by("_users", "email", email).is_some() {
        return Response::bad_request("Email already registered");
    }

    let mut doc = Document::new();
    doc.insert("email".into(), Value::String(email.into()));
    doc.insert("password".into(), Value::String(crypto::hash_password(password)));
    doc.insert("role".into(), Value::String(role.into()));

    match db::get().insert("_users", doc) {
        Some(id) => Response::created(&format!(r#"{{"id":"{}"}}"#, id)),
        None => Response::bad_request("Failed to create user"),
    }
}

pub fn update_user(req: &Request, id: &str) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let json = parse_json(&req.body);

    let email = json.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
    let role = json.get("role").and_then(|v| v.as_str()).map(|s| s.to_string());
    let password = json.get("password").and_then(|v| v.as_str()).map(|s| s.to_string());

    if let Some(email) = &email {
        if !valid_email(email) {
            return Response::bad_request("Invalid email");
        }
        if let Some(existing) = db::get().find_by("_users", "email", email) {
            if existing.get("id").and_then(|v| v.as_str()) != Some(id) {
                return Response::bad_request("Email already registered");
            }
        }
    }

    if let Some(role) = &role {
        if !valid_role(role) {
            return Response::bad_request("Invalid role");
        }
    }

    let mut updates = Document::new();
    if let Some(email) = email {
        updates.insert("email".into(), Value::String(email));
    }
    if let Some(role) = role {
        updates.insert("role".into(), Value::String(role));
    }
    if let Some(password) = password {
        if !valid_password(&password) {
            return Response::bad_request("Password must be at least 8 characters");
        }
        updates.insert("password".into(), Value::String(crypto::hash_password(&password)));
    }

    if updates.is_empty() {
        return Response::bad_request("No updates provided");
    }

    if db::get().update("_users", id, updates) {
        Response::ok(&format!(r#"{{"id":"{}","updated":true}}"#, id))
    } else {
        Response::not_found()
    }
}

pub fn delete_user(req: &Request, id: &str) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    if auth::validate_token(&get_token(req)).as_deref() == Some(id) {
        return Response::bad_request("Cannot delete your own user");
    }
    if db::get().delete("_users", id) {
        Response::ok(r#"{"deleted":true}"#)
    } else {
        Response::not_found()
    }
}

// ── Settings ─────────────────────────────────────────────────────────────────

pub fn get_settings(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let settings = db::get().find_all("_settings");
    if let Some(doc) = settings.first() {
        if let Some(id) = doc.get("id").and_then(|v| v.as_str()) {
            return Response::ok(&format!(
                r#"{{"id":"{}","settings":{}}}"#,
                id,
                db::doc_to_json_for_collection("_settings", doc)
            ));
        }
    }
    Response::ok(r#"{"id":"","settings":{}}"#)
}

pub fn update_settings(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let json = parse_json(&req.body);
    let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let updates = filter_settings(&json);

    if updates.is_empty() {
        return Response::bad_request("No settings provided");
    }

    if !id.is_empty() {
        if db::get().update("_settings", id, updates.clone()) {
            return Response::ok(&format!(r#"{{"id":"{}","updated":true}}"#, id));
        }
    }

    match db::get().insert("_settings", updates) {
        Some(new_id) => Response::created(&format!(r#"{{"id":"{}"}}"#, new_id)),
        None => Response::bad_request("Failed to save settings"),
    }
}

fn filter_settings(doc: &Document) -> Document {
    let mut out = Document::new();
    for key in [
        "page_title",
        "meta_description",
        "meta_keywords",
        "og_title",
        "og_description",
        "og_image",
        "twitter_card",
        "canonical_url",
        "nginx_hostname",
        "nginx_internal_ip",
        "dev_network_name",
        "dev_network_subnet",
        "dev_ip_base",
        "prod_network_name",
        "prod_network_subnet",
        "prod_ip_base",
        "app_port",
        "dev_port_start",
        "dev_port_end",
        "prod_port_start",
        "prod_port_end",
    ] {
        if let Some(value) = doc.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    out
}
