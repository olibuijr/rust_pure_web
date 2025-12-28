# Rust Pure Web

A zero-dependency web framework with built-in database, authentication, and admin panel. No external crates. No npm. Just pure Rust.

## Single Source of Truth

All contributor, agent, and project guidance lives in this `README.md`. Other agent files (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`) exist only to point here and should not be edited.

## Agent Guidance Policy

Do not add agent or contributor guidance to any `.md` file other than `README.md`. If a change is needed for AI agent apps, update `README.md` only.

**After making code changes, always build and apply:**
```bash
cargo build --release && sudo systemctl restart olibuijr-rust
```

**Zero-warning policy:** The build must complete with zero warnings. Fix any warnings immediately—they will become bugs later.

## Integration Testing

The project includes a zero-dependency integration test runner (`src/bin/healthcheck.rs`) that verifies the platform works after deployment.

**Run after every deployment:**
```bash
./target/release/healthcheck              # reads creds from .env.local
./target/release/healthcheck host:port    # test remote server
```

**Test coverage (13 tests):**

| Category | Tests |
|----------|-------|
| Static pages | `GET /` returns 200, `GET /_admin` returns 200, `GET /styles.css` returns 200, 404 handling |
| Auth API | Unauthorized access returns 401, bad credentials return 400, valid login returns token |
| Collections API | Requires auth, lists collections, CRUD operations |
| Admin API | Stats endpoint with admin token |
| E-commerce batch | Full lifecycle test (see below) |

**E-commerce batch test:**
1. Creates 5 test collections: `test_categories`, `test_products`, `test_customers`, `test_orders`, `test_reviews`
2. Inserts sample documents into each collection
3. Verifies documents exist via GET requests
4. Verifies collections appear in listing
5. Cleans up all test data (documents + collections)
6. Verifies cleanup succeeded

The test reads `ADMIN_EMAIL` and `ADMIN_PASSWORD` from `.env.local` automatically. Returns exit code 0 on success, 1 on failure (CI/CD ready).

## Styles (Tailwind CSS Regeneration)

`public/styles.css` is a prebuilt Tailwind output committed to the repo. The only npm dependency is the Tailwind CLI (v4) used for regenerating this file; runtime remains zero-dependency. Keep the generated tooling files (`node_modules`, `package.json`, `package-lock.json`, `public/input.css`) in the repo for future builds.

**TLS exception:** The reverse proxy uses `rustls` to terminate HTTPS with a self-signed certificate. This is the only non-std Rust dependency and is explicitly allowed.

**Reverse Proxy (HTTPS):**
- Cert/key paths: `certs/server.crt`, `certs/server.key`
- Hosts:
  - `olibuijr.com`, `www.olibuijr.com` → base app (public)
  - `dev-$project.olibuijr.com` → dev port for project (admin auth)
  - `$project.olibuijr.com` → prod port for project (admin auth)

To regenerate CSS (only when needed):

```bash
# install tooling temporarily
npm init -y
npm install -D tailwindcss@4.1.18 @tailwindcss/cli@4.1.18

# create config + input
cat > public/input.css <<'EOF'
@import "tailwindcss";

@source "./public/**/*.html";

@theme {
  --color-border: hsl(240 3.7% 15.9%);
  --color-input: hsl(240 3.7% 15.9%);
  --color-ring: hsl(240 4.9% 83.9%);
  --color-background: hsl(240 10% 3.9%);
  --color-foreground: hsl(0 0% 98%);
  --color-primary: hsl(0 0% 98%);
  --color-primary-foreground: hsl(240 5.9% 10%);
  --color-secondary: hsl(240 3.7% 15.9%);
  --color-secondary-foreground: hsl(0 0% 98%);
  --color-muted: hsl(240 3.7% 15.9%);
  --color-muted-foreground: hsl(240 5% 64.9%);
  --color-accent: hsl(240 3.7% 15.9%);
  --color-accent-foreground: hsl(0 0% 98%);
  --color-card: hsl(240 10% 3.9%);
  --color-card-foreground: hsl(0 0% 98%);
}

@layer base {
  body {
    @apply bg-background text-foreground;
    background-image:
      radial-gradient(1200px 700px at 20% -10%, rgba(255, 255, 255, 0.18), transparent 60%),
      radial-gradient(1000px 600px at 80% 0%, rgba(255, 255, 255, 0.14), transparent 55%),
      radial-gradient(900px 700px at 50% 120%, rgba(255, 255, 255, 0.12), transparent 60%),
      linear-gradient(180deg, rgba(15, 23, 42, 0.55), rgba(2, 6, 23, 0.7));
    background-size: 120% 120%, 140% 140%, 160% 160%, 100% 100%;
    background-position: 0% 0%, 100% 0%, 50% 100%, 0% 0%;
    background-attachment: fixed;
  }

  body::before {
    content: "";
    position: fixed;
    inset: -20%;
    z-index: -1;
    background:
      radial-gradient(800px 600px at 20% 0%, rgba(255, 255, 255, 0.12), transparent 60%),
      radial-gradient(700px 500px at 80% 10%, rgba(255, 255, 255, 0.1), transparent 60%),
      radial-gradient(900px 700px at 50% 100%, rgba(255, 255, 255, 0.08), transparent 60%);
    filter: blur(20px);
    transform: translateY(-10%);
    animation: lava 36s ease-in-out infinite;
  }
}

@keyframes lava {
  0% { transform: translateY(-10%); }
  50% { transform: translateY(12%); }
  100% { transform: translateY(-10%); }
}
EOF

# build
node node_modules/@tailwindcss/cli/dist/index.mjs -i ./public/input.css -o ./public/styles.css --minify

# tooling kept for future builds
```

The templates include `/styles.css` and do not use the CDN in production.

## Operations & Logging

Logging is centralized in `src/logging.rs` and writes to `logs.log` in the repo root. Logs are reset on every service start/restart via the systemd unit.

Optional controls:
- `LOG_ENABLED=0` disables logging.
- `LOG_PATH=relative/path.log` changes the log file path (relative to repo root).

## Runtime Root

The server derives the repository root from the executable path, or uses `RPW_ROOT` if set. The systemd service sets `RPW_ROOT` to keep paths stable.

## API Structure

API routes are organized by path, similar to Next.js:

```
src/api/
├── mod.rs          # Routing + Request/Response types
├── admin.rs        # Admin handlers (stats, users, settings)
├── auth.rs         # Authentication handlers
├── collections.rs  # Collection CRUD handlers
├── json.rs         # Zero-dependency JSON parser
└── utils.rs        # Shared utilities + validation re-exports
```

Validation helpers (`valid_email`, `valid_password`, `valid_role`) are defined in `src/auth.rs` and re-exported through `api/utils.rs` for a single source of truth.

## Realtime & WebSocket

Realtime updates are broadcast over WebSocket at `/realtime?token=...` (admin token required). DB writes emit events like:
`doc.created`, `doc.updated`, `doc.deleted`, `collection.created`, `collection.deleted`.

## Admin Settings

Admin → Settings → General stores SEO/meta fields in the `settings` collection. These values are applied across all pages (title postfix, meta, OpenGraph, Twitter, canonical).

## Refresh Persistence

The UI persists state across refreshes via a compact script in `public/templates/components/state-persist.html` (69 lines). Uses a data-driven handlers pattern for:

- **Inputs** - Form values, checkboxes, selects (excludes passwords)
- **Visibility** - Modals and toggles marked with `data-persist="modal"` or `data-persist="toggle"`
- **Focus** - Active element and cursor position
- **Scroll** - Window scroll position and URL hash
- **Custom Data** - Specialized UI states like Admin Chat history

State is stored in localStorage with page-specific keys (`rpw:{path}:{type}`).

## Shared UI Components

Shadcn-style HTML partials are available under `public/templates/components/shadcn/` for reuse with the template engine.

**Mandatory UI Consistency:** Never write raw UI components if a Shadcn-style equivalent exists in `public/templates/components/shadcn/`. Always match the styling (borders, backgrounds, shadows, focus rings) of these components for any new UI elements.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Zero Dependencies](https://img.shields.io/badge/Dependencies-0-brightgreen.svg)](#zero-dependency-philosophy)
[![Lines of Code](https://img.shields.io/badge/Lines-1.8k-blue.svg)](#project-structure)

## Features

- **Zero Dependencies** - Built entirely with Rust's standard library
- **In-Memory Database** - Document store with encrypted file sync
- **Military-Grade Crypto** - SHA-256, PBKDF2, HMAC, ChaCha20 (all pure Rust)
- **Authentication** - Session-based auth with secure password hashing
- **Protected Admin Panel** - SQL browser, API reference, collection management
- **REST API** - Auto-generated CRUD for all collections
- **Modular Components** - Next.js-style reusable component architecture
- **Layout System** - Nested layouts with prop passing via Context
- **Hot Reload** - Automatic browser refresh on file changes
- **Integration Tests** - Zero-dependency test runner with e-commerce batch tests

## Quick Start

```bash
# Clone and build
git clone https://github.com/olibuijr/rust_pure_web.git
cd rust_pure_web

# Configure (optional - defaults provided)
cp .env.local.example .env.local

# Build and run
cargo build --release
./target/release/rust_pure_web
```

- **App**: http://localhost:3460
- **Admin**: http://localhost:3460/_admin
- **Docs**: http://localhost:3460/docs
- **API**: http://localhost:3460/api/

## Configuration

Create `.env.local`:
```env
# Generate: openssl rand -hex 32
SECRET_KEY="your-256-bit-secret-key"

# Default admin (created on first startup)
ADMIN_EMAIL="admin@example.com"
ADMIN_PASSWORD="your-secure-password"
```

## Project Structure

```
rust_pure_web/              # ~1,800 lines Rust
├── src/
│   ├── main.rs             # Entry point, env loading
│   ├── server.rs           # TCP server (9 lines)
│   ├── handler.rs          # HTTP routing
│   ├── api/                # REST API module
│   │   ├── mod.rs          # Routing + Request/Response
│   │   ├── admin.rs        # Admin handlers
│   │   ├── auth.rs         # Auth handlers
│   │   ├── collections.rs  # Collection CRUD
│   │   ├── json.rs         # JSON parser
│   │   └── utils.rs        # Shared utilities
│   ├── auth.rs             # Authentication + validation
│   ├── db.rs               # In-memory database
│   ├── crypto.rs           # SHA-256, PBKDF2, ChaCha20
│   ├── template.rs         # Template engine (79 lines)
│   ├── pages.rs            # Page definitions
│   └── bin/
│       └── healthcheck.rs  # Integration test runner
├── public/templates/
│   ├── layouts/            # Next.js-style layouts
│   │   ├── root.html       # Base HTML structure
│   │   └── docs.html       # Docs layout (extends root)
│   ├── components/         # Reusable components
│   │   ├── nav.html        # Navigation
│   │   ├── footer.html     # Footer
│   │   ├── state-persist.html  # Refresh persistence script
│   │   └── admin/          # Admin panel components (9 files)
│   │       ├── login.html
│   │       ├── nav.html           # Includes title
│   │       ├── sidebar.html
│   │       ├── dashboard.html
│   │       ├── collections.html   # Includes auth guide + create modal
│   │       ├── collection-detail.html  # Includes API ref + doc modal
│   │       ├── settings.html
│   │       ├── users.html
│   │       └── scripts.html
│   ├── docs/               # Documentation pages
│   └── admin.html          # Admin entry (31 lines)
├── data/db.bin             # Encrypted database
└── .env.local              # Configuration
```

## Security

All cryptography implemented in pure Rust following official specifications:

| Algorithm | Standard | Purpose |
|-----------|----------|---------|
| SHA-256 | FIPS 180-4 | Hashing |
| HMAC-SHA256 | RFC 2104 | Message authentication |
| PBKDF2 | RFC 8018 | Password hashing (100k iterations) |
| ChaCha20 | RFC 8439 | Database encryption |

## Database

In-memory document store with automatic encrypted sync:

- JSON-like documents with auto-generated IDs
- Created/updated timestamps
- Binary format for speed
- ChaCha20-256 encryption at rest
- Automatic backup support
- **Reserved collections** - Always preserve `users` and `settings`. They are core system collections and should never be deleted.

## API Endpoints

### Authentication
```
POST /api/auth/register  { email, password }  → { token, user_id }
POST /api/auth/login     { email, password }  → { token, user_id }
POST /api/auth/logout                         → { success }
GET  /api/auth/me                             → { user }
```

### Collections (requires auth)
```
GET    /api/collections              → List collections
POST   /api/collections              → Create collection (admin)
DELETE /api/collections/:name        → Delete collection (admin)
GET    /api/collections/:name        → List documents
POST   /api/collections/:name        → Create document
GET    /api/collections/:name/:id    → Get document
PUT    /api/collections/:name/:id    → Update document
DELETE /api/collections/:name/:id    → Delete document
```

### Admin (requires admin role)
```
GET  /api/admin/stats    → { collections, users }
POST /api/admin/backup   → { backup: "path" }
```

## Template System

### Variables & Includes
```html
{{ variable }}
{% include "component.html" %}
```

### Loops & Conditionals
```html
{% for item in items %}
  {{ item.name }}
{% endfor %}

{% if condition %}...{% else %}...{% endif %}
```

### Layout System (Next.js-style)

Root layout (`layouts/root.html`):
```html
<!DOCTYPE html>
<html>
<head><title>{{ page_title }}</title></head>
<body>
    {{ children }}
</body>
</html>
```

Nested layout (`layouts/docs.html`):
```html
{% layout "layouts/root.html" %}
<div class="sidebar">...</div>
<main>{{ children }}</main>
```

Page using layout:
```html
{% layout "layouts/docs.html" %}
<h1>Page Content</h1>
```

Props flow through all layouts via Context:
```rust
let mut ctx = Context::new();
ctx.set("page_title", "My Page");
template::render(&content, &ctx);
```

### Modular Components

Components are small, reusable HTML files:
```html
<!-- components/admin/nav.html -->
<nav class="border-b">
    <span>Admin Panel</span>
    <button onclick="logout()">Logout</button>
</nav>
```

Include in any page:
```html
{% include "components/admin/nav.html" %}
```

This keeps pages minimal. For example, `admin.html` is just 31 lines:
```html
<!DOCTYPE html>
<html>
<body>
    {% include "components/admin/login.html" %}
    <div id="admin-panel">
        {% include "components/admin/nav.html" %}
        {% include "components/admin/sidebar.html" %}
        {% include "components/admin/dashboard.html" %}
        {% include "components/admin/collections.html" %}
    </div>
    {% include "components/admin/scripts.html" %}
</body>
</html>
```

## Admin Panel

Access at `/_admin`. Features:

- **Dashboard** - Collection/user counts, backup creation
- **SQL Browser** - Table view of documents per collection
- **API Reference** - Auto-generated curl examples per collection
- **Authentication Guide** - How to use the REST API with tokens
- **User Management** - View all registered users
- **Servers** - Track Nginx proxy + internal network details
- **Port Control** - View app port and dev/prod ranges for projects
- **Settings** - Manage site metadata and infrastructure defaults

## Systemd Service

```bash
sudo cp olibuijr-rust.service /etc/systemd/system/
sudo systemctl enable olibuijr-rust
sudo systemctl start olibuijr-rust
```

## Project Environments (Dev → Prod)

Project lifecycle in the admin UI:

- **Create Project** → creates dev collections prefixed with `dev-{project}_*` and a default dev login.
- **Delete Project** → removes the project files, collections, and port assignments.

Routing:
- `https://dev-$project.olibuijr.com` → dev port for the project
- `https://$project.olibuijr.com` → prod port for the project

## Zero-Dependency Philosophy

| Feature | Pure Rust Implementation |
|---------|-------------------------|
| HTTP Server | `std::net::TcpListener` |
| HTTP Client | `std::net::TcpStream` (for tests) |
| Database | `HashMap` + binary serialization |
| Encryption | Custom ChaCha20 |
| Password Hash | Custom PBKDF2-SHA256 |
| JSON Parser | Custom recursive descent |
| Sessions | Random tokens from `/dev/urandom` |
| Templates | Custom parser with layouts |
| Hot Reload | File mtime polling |
| Integration Tests | Custom HTTP client + assertions |

**No tokio. No hyper. No serde. No reqwest. Just `std`.**

## Documentation

Full documentation available at http://localhost:3460/docs

- [Introduction](/docs)
- [Authentication](/docs/authentication)
- [Database](/docs/database)
- [API Reference](/docs/api)
- [Cryptography](/docs/crypto)
- [Templates](/docs/templates)

---

Built with Rust from Akureyri, Iceland
