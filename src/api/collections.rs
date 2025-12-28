use crate::api::{Request, Response};
use crate::api::json::{parse_json, JsonSerializer as Json};
use crate::api::utils::{is_private_collection, require_admin, require_auth};
use crate::db::{self, Value};

pub fn list_collections(req: &Request) -> Response {
    if !require_auth(req) { return Response::unauthorized(); }
    let cols = db::get().list_collections();
    Response::ok(&format!(r#"{{"collections":[{}]}}"#,
        cols.iter().map(|c| Json::wrap_string(c)).collect::<Vec<_>>().join(",")
    ))
}

pub fn list_system_collections(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let cols = db::get().list_all_collections();
    let system: Vec<String> = cols
        .into_iter()
        .filter(|c| c.starts_with('_'))
        .collect();
    Response::ok(&format!(r#"{{"collections":[{}]}}"#,
        system.iter().map(|c| Json::wrap_string(c)).collect::<Vec<_>>().join(",")
    ))
}

pub fn create_collection(req: &Request) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    let json = parse_json(&req.body);
    let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("");

    if name.is_empty() || name.starts_with('_') {
        return Response::bad_request("Invalid collection name");
    }

    let fields: Vec<(String, String)> = json.get("fields")
        .and_then(|v| match v { Value::Array(arr) => Some(arr), _ => None })
        .map(|arr| arr.iter().filter_map(|f| {
            let obj = f.as_object()?;
            let name = obj.get("name")?.as_str()?;
            let typ = obj.get("type")?.as_str().unwrap_or("string");
            Some((name.to_string(), typ.to_string()))
        }).collect())
        .unwrap_or_default();

    db::get().create_collection(name, fields);
    Response::created(&format!(r#"{{"name":"{}"}}"#, name))
}

pub fn delete_collection(req: &Request, name: &str) -> Response {
    if !require_admin(req) { return Response::unauthorized(); }
    if db::get().delete_collection(name) {
        Response::ok(r#"{"deleted":true}"#)
    } else {
        Response::bad_request("Cannot delete this collection")
    }
}

pub fn list_documents(req: &Request, collection: &str) -> Response {
    if !require_auth(req) { return Response::unauthorized(); }
    if is_private_collection(collection) && !require_admin(req) { return Response::unauthorized(); }
    let docs = db::get().find_all(collection);
    let json: Vec<String> = docs.iter().map(|d| db::doc_to_json_for_collection(collection, d)).collect();
    Response::ok(&format!("[{}]", json.join(",")))
}

pub fn create_document(req: &Request, collection: &str) -> Response {
    if !require_auth(req) { return Response::unauthorized(); }
    if is_private_collection(collection) && !require_admin(req) { return Response::unauthorized(); }
    let doc = parse_json(&req.body);
    match db::get().insert(collection, doc) {
        Some(id) => Response::created(&format!(r#"{{"id":"{}"}}"#, id)),
        None => Response::bad_request("Failed to create document"),
    }
}

pub fn get_document(req: &Request, collection: &str, id: &str) -> Response {
    if !require_auth(req) { return Response::unauthorized(); }
    if is_private_collection(collection) && !require_admin(req) { return Response::unauthorized(); }
    match db::get().find_one(collection, id) {
        Some(doc) => Response::ok(&db::doc_to_json_for_collection(collection, &doc)),
        None => Response::not_found(),
    }
}

pub fn update_document(req: &Request, collection: &str, id: &str, body: &str) -> Response {
    if !require_auth(req) { return Response::unauthorized(); }
    if is_private_collection(collection) && !require_admin(req) { return Response::unauthorized(); }
    let updates = parse_json(body);
    if db::get().update(collection, id, updates) {
        Response::ok(&format!(r#"{{"id":"{}","updated":true}}"#, id))
    } else {
        Response::not_found()
    }
}

pub fn delete_document(req: &Request, collection: &str, id: &str) -> Response {
    if !require_auth(req) { return Response::unauthorized(); }
    if is_private_collection(collection) && !require_admin(req) { return Response::unauthorized(); }
    if db::get().delete(collection, id) {
        Response::ok(r#"{"deleted":true}"#)
    } else {
        Response::not_found()
    }
}
