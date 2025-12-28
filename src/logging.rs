use crate::config;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();
static LOG_ENABLED: OnceLock<bool> = OnceLock::new();
static LOG_PATH: OnceLock<String> = OnceLock::new();

pub fn init() {
    let enabled = env::var("LOG_ENABLED").map(|v| v != "0" && v.to_lowercase() != "false").unwrap_or(true);
    let _ = LOG_ENABLED.set(enabled);

    if !enabled {
        return;
    }

    let path = env::var("LOG_PATH")
        .map(|p| config::root_dir().join(p))
        .unwrap_or_else(|_| config::root_dir().join("logs.log"));
    let _ = LOG_PATH.set(path.to_string_lossy().to_string());
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path);
    if let Ok(file) = file {
        let _ = LOG_FILE.set(Mutex::new(file));
        info("logging", "log file initialized");
    } else {
        eprintln!("WARNING: failed to open logs.log for writing");
    }
}

pub fn info(scope: &str, message: &str) {
    write("INFO", scope, message);
}

pub fn warn(scope: &str, message: &str) {
    write("WARN", scope, message);
}

pub fn error(scope: &str, message: &str) {
    write("ERROR", scope, message);
}

fn write(level: &str, scope: &str, message: &str) {
    if !*LOG_ENABLED.get_or_init(|| true) {
        return;
    }
    let ts = timestamp();
    if let Some(lock) = LOG_FILE.get() {
        if let Ok(mut file) = lock.lock() {
            let _ = writeln!(file, "{} [{}] {} - {}", ts, level, scope, message);
        }
    } else {
        eprintln!("{} [{}] {} - {}", ts, level, scope, message);
    }
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

pub fn log_path() -> Option<String> {
    LOG_PATH.get().cloned()
}
