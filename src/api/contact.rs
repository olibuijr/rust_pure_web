use crate::api::{Request, Response};
use crate::api::json::parse_json;
use crate::api::utils::valid_email;
use crate::db::{self, Value};

const CONTACT_COLLECTION: &str = "contact_messages";
const MAX_NAME_LEN: usize = 120;
const MAX_EMAIL_LEN: usize = 320;
const MAX_MESSAGE_LEN: usize = 2000;
const MIN_ELAPSED_SECS: i64 = 3;
const MAX_ELAPSED_SECS: i64 = 60 * 60;

pub fn submit(req: &Request) -> Response {
    let payload = parse_json(&req.body);
    let company = payload.get("company").and_then(|v| v.as_str()).unwrap_or("").trim();
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
    let email = payload.get("email").and_then(|v| v.as_str()).unwrap_or("").trim();
    let message = payload.get("message").and_then(|v| v.as_str()).unwrap_or("").trim();
    let elapsed = payload.get("elapsed").and_then(read_int).unwrap_or(0);

    if !company.is_empty() {
        return Response::bad_request("Bot detected");
    }
    if elapsed < MIN_ELAPSED_SECS || elapsed > MAX_ELAPSED_SECS {
        return Response::bad_request("Please wait a moment before submitting");
    }

    if name.is_empty() || email.is_empty() || message.is_empty() {
        return Response::bad_request("Name, email, and message are required");
    }
    if name.len() > MAX_NAME_LEN {
        return Response::bad_request("Name is too long");
    }
    if email.len() > MAX_EMAIL_LEN || !valid_email(email) {
        return Response::bad_request("Invalid email");
    }
    if message.len() > MAX_MESSAGE_LEN {
        return Response::bad_request("Message is too long");
    }

    ensure_contact_collection();

    let mut doc = std::collections::HashMap::new();
    doc.insert("name".into(), Value::String(name.to_string()));
    doc.insert("email".into(), Value::String(email.to_string()));
    doc.insert("message".into(), Value::String(message.to_string()));

    match db::get().insert(CONTACT_COLLECTION, doc) {
        Some(id) => Response::created(&format!(r#"{{"id":"{}"}}"#, id)),
        None => Response::bad_request("Failed to save message"),
    }
}

fn read_int(value: &Value) -> Option<i64> {
    match value {
        Value::Int(i) => Some(*i),
        Value::Float(f) => Some(*f as i64),
        Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

pub fn ensure_contact_collection() {
    let exists = db::get()
        .list_all_collections()
        .iter()
        .any(|c| c == CONTACT_COLLECTION);
    if !exists {
        db::get().create_collection(
            CONTACT_COLLECTION,
            vec![
                ("name".into(), "string".into()),
                ("email".into(), "string".into()),
                ("message".into(), "string".into()),
            ],
        );
    }
}
