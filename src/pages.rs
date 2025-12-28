//! Page definitions with type-safe templates
use crate::template::{self, Context};
use crate::db;

/// Skill data
pub struct Skill {
    pub name: &'static str,
    pub icon: &'static str,
    pub category: &'static str,
}

/// Project data
pub struct Project {
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub gradient: &'static str,
    pub tags: &'static str,
}

/// Index page
pub struct IndexPage {
    pub name: &'static str,
    pub title: &'static str,
    pub location: &'static str,
    pub email: &'static str,
    pub available: bool,
    pub skills: Vec<Skill>,
    pub projects: Vec<Project>,
}

impl IndexPage {
    pub fn render(&self) -> String {
        let mut ctx = settings_context("Home");
        let mut html = template::load("index.html");

        // Conditionals
        html = template::process_conditionals(html, "available", self.available);

        // Loops
        html = template::process_loop(&html, "skill", &self.skills, |s| {
            vec![("name", s.name), ("icon", s.icon), ("category", s.category)]
        });
        html = template::process_loop(&html, "project", &self.projects, |p| {
            vec![("name", p.name), ("description", p.description),
                 ("icon", p.icon), ("gradient", p.gradient), ("tags", p.tags)]
        });

        ctx.set("name", self.name);
        ctx.set("title", self.title);
        ctx.set("location", self.location);
        ctx.set("email", self.email);

        template::render(&html, &ctx)
    }

    
}

/// Documentation page
pub struct DocsPage {
    pub page_title: &'static str,
    pub content_file: &'static str,
    pub active_page: &'static str,
}

impl DocsPage {
    pub fn render(&self) -> String {
        let mut ctx = settings_context(self.page_title);

        // Active states
        for page in ["intro", "installation", "quickstart", "zerodep", "arch", "server",
                     "templates", "vars", "loops", "cond", "includes", "comp", "style",
                     "routing", "hotreload", "deploy", "auth", "database", "api", "crypto"] {
            ctx.set(&format!("active_{}", page),
                if page == self.active_page { "text-foreground font-medium" } else { "" });
        }

        let content = template::load(&format!("docs/{}", self.content_file));
        let with_layout = format!("{{% layout \"layouts/docs.html\" %}}\n{}", content);
        template::render(&with_layout, &ctx)
    }
}

pub fn settings_context(page_title: &str) -> Context {
    let mut ctx = Context::new();
    let settings = db::get().find_all("_settings");
    let data = settings.first();

    let site_title = get_setting(data, "page_title").unwrap_or("Rust Pure Web".to_string());
    let meta_description = get_setting(data, "meta_description").unwrap_or_default();
    let meta_keywords = get_setting(data, "meta_keywords").unwrap_or_default();
    let og_title = get_setting(data, "og_title")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{} | {}", page_title, site_title));
    let og_description = get_setting(data, "og_description")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| meta_description.clone());
    let og_image = get_setting(data, "og_image").unwrap_or_default();
    let twitter_card = get_setting(data, "twitter_card").unwrap_or_default();
    let canonical_url = get_setting(data, "canonical_url").unwrap_or_default();
    let canonical_tag = if canonical_url.is_empty() {
        String::new()
    } else {
        format!(r#"<link rel="canonical" href="{}">"#, canonical_url)
    };

    ctx.set("page_title", page_title);
    ctx.set("site_title", &site_title);
    ctx.set("meta_description", &meta_description);
    ctx.set("meta_keywords", &meta_keywords);
    ctx.set("og_title", &og_title);
    ctx.set("og_description", &og_description);
    ctx.set("og_image", &og_image);
    ctx.set("twitter_card", &twitter_card);
    ctx.set("canonical_url", &canonical_url);
    ctx.set_raw("canonical_tag", &canonical_tag);
    ctx.set("body_class", "");

    ctx
}

fn get_setting(doc: Option<&db::Document>, key: &str) -> Option<String> {
    doc.and_then(|d| d.get(key)).and_then(value_to_string)
}

fn value_to_string(value: &db::Value) -> Option<String> {
    match value {
        db::Value::String(s) => Some(s.clone()),
        db::Value::Int(i) => Some(i.to_string()),
        db::Value::Float(f) => Some(f.to_string()),
        db::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Docs page factory
pub fn docs(slug: &str) -> DocsPage {
    match slug {
        "" | "index" => DocsPage { page_title: "Introduction", content_file: "intro.html", active_page: "intro" },
        "zero-dependency" => DocsPage { page_title: "Zero Dependencies", content_file: "zero-dependency.html", active_page: "zerodep" },
        "templates" => DocsPage { page_title: "Template Engine", content_file: "templates.html", active_page: "templates" },
        "installation" => DocsPage { page_title: "Installation", content_file: "installation.html", active_page: "installation" },
        "quick-start" => DocsPage { page_title: "Quick Start", content_file: "quick-start.html", active_page: "quickstart" },
        "architecture" => DocsPage { page_title: "Architecture", content_file: "architecture.html", active_page: "arch" },
        "server" => DocsPage { page_title: "HTTP Server", content_file: "server.html", active_page: "server" },
        "variables" => DocsPage { page_title: "Variables", content_file: "variables.html", active_page: "vars" },
        "loops" => DocsPage { page_title: "Loops", content_file: "loops.html", active_page: "loops" },
        "conditionals" => DocsPage { page_title: "Conditionals", content_file: "conditionals.html", active_page: "cond" },
        "includes" => DocsPage { page_title: "Includes", content_file: "includes.html", active_page: "includes" },
        "components" => DocsPage { page_title: "Components", content_file: "components.html", active_page: "comp" },
        "styling" => DocsPage { page_title: "Styling", content_file: "styling.html", active_page: "style" },
        "routing" => DocsPage { page_title: "Routing", content_file: "routing.html", active_page: "routing" },
        "hot-reload" => DocsPage { page_title: "Hot Reload", content_file: "hot-reload.html", active_page: "hotreload" },
        "deployment" => DocsPage { page_title: "Deployment", content_file: "deployment.html", active_page: "deploy" },
        "authentication" => DocsPage { page_title: "Authentication", content_file: "auth.html", active_page: "auth" },
        "database" => DocsPage { page_title: "Database", content_file: "database.html", active_page: "database" },
        "api" => DocsPage { page_title: "API Reference", content_file: "api.html", active_page: "api" },
        "crypto" => DocsPage { page_title: "Cryptography", content_file: "crypto.html", active_page: "crypto" },
        _ => DocsPage { page_title: "Not Found", content_file: "intro.html", active_page: "intro" },
    }
}

/// Default index page data
pub fn index() -> IndexPage {
    IndexPage {
        name: "Ólafur Búi Ólafsson",
        title: "Web Developer",
        location: "Akureyri, Iceland",
        email: "olibuijr@olibuijr.com",
        available: true,
        skills: vec![
            Skill { name: "React", icon: "⚛️", category: "Frontend" },
            Skill { name: "Node.js", icon: "🟢", category: "Backend" },
            Skill { name: "Rust", icon: "🦀", category: "Systems" },
            Skill { name: "PostgreSQL", icon: "🗄️", category: "Database" },
            Skill { name: "Tailwind", icon: "🎨", category: "Styling" },
            Skill { name: "Svelte", icon: "📱", category: "Frontend" },
            Skill { name: "AWS", icon: "☁️", category: "Cloud" },
            Skill { name: "Docker", icon: "🐳", category: "DevOps" },
        ],
        projects: vec![
            Project { name: "Arctic Tours Platform", description: "Booking platform for northern Iceland tours.", icon: "🌊", gradient: "from-blue-600 to-cyan-500", tags: "React, Node.js, PostgreSQL" },
            Project { name: "Fishery Management System", description: "Real-time tracking for fishing companies in North Iceland.", icon: "🐟", gradient: "from-purple-600 to-pink-500", tags: "Rust, Svelte, WebSocket" },
            Project { name: "Eco Iceland", description: "Sustainability tracking app for Icelandic businesses.", icon: "🌿", gradient: "from-green-600 to-emerald-500", tags: "Next.js, Tailwind, Prisma" },
            Project { name: "Volcano Alert", description: "Real-time volcanic activity monitoring dashboard.", icon: "🌋", gradient: "from-orange-600 to-yellow-500", tags: "React, WebSocket, AWS" },
        ],
    }
}
