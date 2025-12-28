use crate::auth;
use crate::api::Request;
use crate::config;

// Re-export validation helpers from auth (single source of truth)
pub use crate::auth::{valid_email, valid_password, valid_role};

pub fn load_env(key: &str) -> Option<String> {
    config::load_env(key)
}

pub fn get_token(req: &Request) -> String {
    req.headers.get("authorization")
        .map(|h| h.trim_start_matches("Bearer ").to_string())
        .or_else(|| req.headers.get("cookie")
            .and_then(|c| c.split(';').find(|p| p.trim().starts_with("token=")))
            .map(|p| p.trim().trim_start_matches("token=").to_string()))
        .unwrap_or_default()
}

pub fn require_auth(req: &Request) -> bool {
    auth::validate_token(&get_token(req)).is_some()
}

pub fn require_admin(req: &Request) -> bool {
    auth::is_admin(&get_token(req))
}

pub fn is_private_collection(name: &str) -> bool {
    name.starts_with('_')
}
