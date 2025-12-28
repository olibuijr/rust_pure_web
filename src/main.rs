mod config;
mod logging;
mod api;
mod auth;
mod crypto;
mod db;
mod handler;
mod pages;
mod ports;
mod proxy;
mod server;
mod template;
mod realtime;
mod ws;

fn main() {
    logging::init();
    std::panic::set_hook(Box::new(|info| {
        logging::error("panic", &format!("{}", info));
    }));
    if let Some(path) = logging::log_path() {
        logging::info("logging", &format!("log path: {}", path));
    }
    // Load encryption key from .env.local
    let key = config::load_env("SECRET_KEY").unwrap_or_else(|| {
        eprintln!("WARNING: SECRET_KEY not found in .env.local, using default (insecure!)");
        logging::warn("config", "SECRET_KEY missing, using insecure default");
        "default-insecure-key-change-me".to_string()
    });

    // Initialize database
    db::init(&key);

    // Create default admin if no users exist
    if db::get().find_all("_users").is_empty() {
        if let (Some(email), Some(password)) = (config::load_env("ADMIN_EMAIL"), config::load_env("ADMIN_PASSWORD")) {
            let result = auth::register(&email, &password);
            if result.success {
                println!("Created default admin: {}", email);
                logging::info("auth", "default admin created");
            } else {
                eprintln!("Failed to create admin: {:?}", result.error);
                logging::error("auth", "failed to create default admin");
            }
        }
    }

    // Seed settings defaults once
    if db::get().find_all("_settings").is_empty() {
        let mut doc = db::Document::new();
        doc.insert("page_title".into(), db::Value::String("Rust Pure Web".into()));
        doc.insert("meta_description".into(), db::Value::String("Zero-dependency Rust web framework.".into()));
        doc.insert("meta_keywords".into(), db::Value::String("rust, web, zero-deps".into()));
        doc.insert("og_title".into(), db::Value::String("Rust Pure Web".into()));
        doc.insert("og_description".into(), db::Value::String("Fast, zero-dependency web framework in Rust.".into()));
        doc.insert("og_image".into(), db::Value::String("".into()));
        doc.insert("twitter_card".into(), db::Value::String("summary_large_image".into()));
        doc.insert("canonical_url".into(), db::Value::String("".into()));
        doc.insert("nginx_hostname".into(), db::Value::String("proxy.olibuijr.com".into()));
        doc.insert("nginx_internal_ip".into(), db::Value::String("192.168.8.4".into()));
        doc.insert("dev_network_name".into(), db::Value::String("dev".into()));
        doc.insert("dev_network_subnet".into(), db::Value::String("10.35.0.0/24".into()));
        doc.insert("dev_ip_base".into(), db::Value::String("10.35.0.".into()));
        doc.insert("prod_network_name".into(), db::Value::String("prod".into()));
        doc.insert("prod_network_subnet".into(), db::Value::String("10.36.0.0/24".into()));
        doc.insert("prod_ip_base".into(), db::Value::String("10.36.0.".into()));
        doc.insert("app_port".into(), db::Value::Int(3460));
        doc.insert("dev_port_start".into(), db::Value::Int(3501));
        doc.insert("dev_port_end".into(), db::Value::Int(3599));
        doc.insert("prod_port_start".into(), db::Value::Int(3601));
        doc.insert("prod_port_end".into(), db::Value::Int(3699));
        let _ = db::get().insert("_settings", doc);
    }

    api::contact::ensure_contact_collection();

    println!("Server listening on http://0.0.0.0:3460");
    println!("Admin panel: http://0.0.0.0:3460/_admin");
    logging::info("server", "listening on 0.0.0.0:3460");
    std::thread::spawn(|| proxy::run_proxy());
    server::run("0.0.0.0:3460");
}
