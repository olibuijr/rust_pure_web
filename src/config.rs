use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

static ROOT_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn root_dir() -> PathBuf {
    ROOT_DIR
        .get_or_init(|| {
            env::var("RPW_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|_| resolve_root_dir())
        })
        .clone()
}

pub fn public_dir() -> PathBuf {
    root_dir().join("public")
}

pub fn templates_dir() -> PathBuf {
    public_dir().join("templates")
}

pub fn data_dir() -> PathBuf {
    root_dir().join("data")
}

pub fn env_path() -> PathBuf {
    root_dir().join(".env.local")
}

pub fn load_env(key: &str) -> Option<String> {
    let content = std::fs::read_to_string(env_path()).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Returns the configured CORS origin.
/// Reads from CORS_ORIGIN environment variable in .env.local
/// Defaults to "*" for development convenience.
/// Production deployments should set a specific origin (e.g., "https://example.com")
pub fn cors_origin() -> String {
    load_env("CORS_ORIGIN").unwrap_or_else(|| "*".to_string())
}

pub fn hot_reload() -> bool {
    // Check system environment variable first, then .env.local file
    env::var("HOT_RELOAD")
        .ok()
        .or_else(|| load_env("HOT_RELOAD"))
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

fn resolve_root_dir() -> PathBuf {
    if let Ok(exe) = env::current_exe() {
        if let Some(release_dir) = exe.parent() {
            if let Some(target_dir) = release_dir.parent() {
                if let Some(repo_dir) = target_dir.parent() {
                    return repo_dir.to_path_buf();
                }
            }
        }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
