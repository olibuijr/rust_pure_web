use crate::{auth, db};
use crate::api::{Request, Response};
use crate::api::json::parse_json;
use crate::api::utils::{get_token};

pub fn register(req: &Request) -> Response {
    let json = parse_json(&req.body);
    let email = json.get("email").and_then(|v| v.as_str()).unwrap_or("");
    let password = json.get("password").and_then(|v| v.as_str()).unwrap_or("");

    let result = auth::register(email, password);
    if result.success {
        Response::created(&format!(
            r#"{{"token":"{}","user_id":"{}"}}"#,
            result.token.unwrap_or_default(),
            result.user_id.unwrap_or_default()
        ))
    } else {
        Response::bad_request(&result.error.unwrap_or_default())
    }
}

pub fn login(req: &Request) -> Response {
    let json = parse_json(&req.body);
    let email = json.get("email").and_then(|v| v.as_str()).unwrap_or("");
    let password = json.get("password").and_then(|v| v.as_str()).unwrap_or("");

    let result = auth::login(email, password);
    if result.success {
        Response::ok(&format!(
            r#"{{"token":"{}","user_id":"{}"}}"#,
            result.token.unwrap_or_default(),
            result.user_id.unwrap_or_default()
        ))
    } else {
        Response::bad_request(&result.error.unwrap_or_default())
    }
}

pub fn logout(req: &Request) -> Response {
    let token = get_token(req);
    auth::logout(&token);
    Response::ok(r#"{"success":true}"#)
}

pub fn me(req: &Request) -> Response {
    let token = get_token(req);
    match auth::get_user(&token) {
        Some(user) => Response::ok(&db::doc_to_json_for_collection("_users", &user)),
        None => Response::unauthorized(),
    }
}
