//! Authentication system - register, login, sessions
use crate::crypto::{hash_password, verify_password, random_hex};
use crate::db::{self, Document, Value};

const SESSION_DURATION: i64 = 86400 * 7; // 7 days

pub struct AuthResult {
    pub success: bool,
    pub token: Option<String>,
    pub user_id: Option<String>,
    pub error: Option<String>,
}

impl AuthResult {
    fn ok(token: String, user_id: String) -> Self {
        Self { success: true, token: Some(token), user_id: Some(user_id), error: None }
    }
    fn err(msg: &str) -> Self {
        Self { success: false, token: None, user_id: None, error: Some(msg.into()) }
    }
}

/// Register new user
pub fn register(email: &str, password: &str) -> AuthResult {
    let db = db::get();

    // Check if email exists
    if db.find_by("_users", "email", email).is_some() {
        return AuthResult::err("Email already registered");
    }

    // Validate
    if !valid_email(email) {
        return AuthResult::err("Invalid email");
    }
    if !valid_password(password) {
        return AuthResult::err("Password must be at least 8 characters");
    }

    // Create user (first user is admin)
    let is_first = db.find_all("_users").is_empty();
    let mut doc = Document::new();
    doc.insert("email".into(), Value::String(email.into()));
    doc.insert("password".into(), Value::String(hash_password(password)));
    doc.insert("role".into(), Value::String(if is_first { "admin" } else { "user" }.into()));

    match db.insert("_users", doc) {
        Some(user_id) => {
            let token = create_session(&user_id);
            AuthResult::ok(token, user_id)
        }
        None => AuthResult::err("Failed to create user"),
    }
}

/// Login user
pub fn login(email: &str, password: &str) -> AuthResult {
    let db = db::get();

    let user = match db.find_by("_users", "email", email) {
        Some(u) => u,
        None => return AuthResult::err("Invalid credentials"),
    };

    let stored_hash = match user.get("password").and_then(|v| v.as_str()) {
        Some(h) => h,
        None => return AuthResult::err("Invalid credentials"),
    };

    if !verify_password(password, stored_hash) {
        return AuthResult::err("Invalid credentials");
    }

    let user_id = match user.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return AuthResult::err("User corrupt"),
    };

    let token = create_session(&user_id);
    AuthResult::ok(token, user_id)
}

/// Create session token
fn create_session(user_id: &str) -> String {
    let db = db::get();
    let token = random_hex(32);
    let expires = db::now() + SESSION_DURATION;

    let mut doc = Document::new();
    doc.insert("user_id".into(), Value::String(user_id.into()));
    doc.insert("token".into(), Value::String(token.clone()));
    doc.insert("expires".into(), Value::Int(expires));

    db.insert("_sessions", doc);
    token
}

/// Validate session token, return user_id if valid
pub fn validate_token(token: &str) -> Option<String> {
    let db = db::get();
    let session = db.find_by("_sessions", "token", token)?;

    let expires = match session.get("expires") {
        Some(Value::Int(e)) => *e,
        _ => return None,
    };

    if expires < db::now() {
        // Expired, delete session
        if let Some(Value::String(id)) = session.get("id") {
            db.delete("_sessions", id);
        }
        return None;
    }

    session.get("user_id").and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Get current user from token
pub fn get_user(token: &str) -> Option<Document> {
    let user_id = validate_token(token)?;
    let db = db::get();
    let mut user = db.find_one("_users", &user_id)?;
    user.remove("password"); // Don't expose password hash
    Some(user)
}

/// Logout - invalidate session
pub fn logout(token: &str) -> bool {
    let db = db::get();
    if let Some(session) = db.find_by("_sessions", "token", token) {
        if let Some(Value::String(id)) = session.get("id") {
            return db.delete("_sessions", id);
        }
    }
    false
}

/// Check if user has admin role
pub fn is_admin(token: &str) -> bool {
    get_user(token)
        .and_then(|u| u.get("role").and_then(|v| v.as_str()).map(|s| s == "admin"))
        .unwrap_or(false)
}

// ── Validation helpers (single source of truth) ─────────────────────────────

pub fn valid_email(email: &str) -> bool {
    email.len() >= 3 && email.contains('@')
}

pub fn valid_password(password: &str) -> bool {
    password.len() >= 8
}

pub fn valid_role(role: &str) -> bool {
    role == "admin" || role == "user"
}
