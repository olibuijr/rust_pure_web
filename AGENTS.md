# AGENTS.md - AI Agent Guidelines

## Project Overview

**Rust Pure Web** is a zero-dependency web framework with built-in database, authentication, and admin panel. No external crates. No npm. Just pure Rust.

- **Author:** Olafur Bui Olafsson (@olibuijr)
- **Location:** Built from Akureyri, Iceland
- **License:** MIT

## Build & Deploy

```bash
# Build
cargo build --release

# Test
cargo test

# Deploy (build + restart service)
cargo build --release && sudo systemctl restart olibuijr-rust
```

**Zero-warning policy:** The build must complete with zero warnings. Fix any warnings immediately.

### Tailwind CSS Regeneration

Rebuild when changing templates with new utility classes:

```bash
node node_modules/@tailwindcss/cli/dist/index.mjs -i ./public/input.css -o ./public/styles.css --minify
```

The tooling files (`node_modules`, `package.json`, `package-lock.json`, `public/input.css`) are committed for future builds.

### Integration Tests

```bash
./target/release/healthcheck              # reads creds from .env.local
./target/release/healthcheck host:port    # test remote server
```

13 tests covering: static pages, auth API, collections API, admin API, e-commerce batch lifecycle.

## Project Structure

```
rust_pure_web/              # ~1,800 lines Rust
├── src/
│   ├── main.rs             # Entry point, env loading
│   ├── server.rs           # TCP server
│   ├── handler.rs          # HTTP routing
│   ├── api/                # REST API module
│   │   ├── mod.rs          # Routing + Request/Response types
│   │   ├── admin.rs        # Admin handlers (stats, users, settings)
│   │   ├── auth.rs         # Authentication handlers
│   │   ├── collections.rs  # Collection CRUD handlers
│   │   ├── json.rs         # Zero-dependency JSON parser
│   │   └── utils.rs        # Shared utilities + validation
│   ├── auth.rs             # Authentication + validation helpers
│   ├── db.rs               # In-memory database
│   ├── crypto.rs           # SHA-256, PBKDF2, ChaCha20
│   ├── template.rs         # Template engine
│   ├── pages.rs            # Page definitions and routes
│   ├── logging.rs          # Centralized logging (writes to logs.log)
│   └── bin/
│       └── healthcheck.rs  # Integration test runner
├── public/
│   ├── templates/
│   │   ├── layouts/        # Next.js-style layouts
│   │   │   ├── root.html   # Base HTML structure
│   │   │   └── docs.html   # Docs layout (extends root)
│   │   ├── components/     # Reusable components
│   │   │   ├── nav.html
│   │   │   ├── footer.html
│   │   │   ├── state-persist.html
│   │   │   ├── shadcn/     # Shadcn-style UI components
│   │   │   └── admin/      # Admin panel components
│   │   ├── docs/           # Documentation pages
│   │   ├── admin.html      # Admin entry point
│   │   └── index.html      # Homepage
│   ├── styles.css          # Compiled Tailwind CSS
│   └── input.css           # Tailwind input config
├── data/
│   └── db.bin              # Encrypted database (ChaCha20)
├── certs/                  # TLS certificates
│   ├── server.crt
│   └── server.key
├── logs.log                # Application logs
└── .env.local              # Configuration (not committed)
```

## Key Files Reference

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, environment loading, server startup |
| `src/handler.rs` | HTTP request routing and response handling |
| `src/pages.rs` | Page definitions, routes, and context setup |
| `src/template.rs` | Template engine (79 lines) |
| `src/db.rs` | In-memory document store with encrypted sync |
| `src/auth.rs` | Authentication, sessions, validation helpers |
| `src/crypto.rs` | SHA-256, PBKDF2, HMAC, ChaCha20 implementations |
| `src/api/mod.rs` | API routing and Request/Response types |
| `src/api/collections.rs` | Collection CRUD handlers |
| `src/api/auth.rs` | Auth endpoint handlers |
| `src/api/admin.rs` | Admin endpoint handlers |
| `src/api/json.rs` | Zero-dependency JSON parser |
| `src/logging.rs` | Centralized logging |

## Template System

### Syntax

| Feature | Syntax |
|---------|--------|
| Variables | `{{ name }}` |
| Loops | `{% for item in items %}...{% endfor %}` |
| Conditionals | `{% if condition %}...{% else %}...{% endif %}` |
| Includes | `{% include "path/file.html" %}` |
| Layouts | `{% layout "layouts/name.html" %}` |

### Layout System (Next.js-style)

Root layout receives `{{ children }}` placeholder. Props flow through all nested layouts via Context.

```html
<!-- Page using layout -->
{% layout "layouts/docs.html" %}
<h1>Page Content</h1>
```

### Components

Reusable HTML partials in `public/templates/components/`. Include with:

```html
{% include "components/admin/nav.html" %}
```

## API Structure

### Routes

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/auth/register` | POST | Register user |
| `/api/auth/login` | POST | Login, get token |
| `/api/auth/logout` | POST | Logout |
| `/api/auth/me` | GET | Current user |
| `/api/collections` | GET | List collections |
| `/api/collections` | POST | Create collection (admin) |
| `/api/collections/:name` | DELETE | Delete collection (admin) |
| `/api/collections/:name` | GET | List documents |
| `/api/collections/:name` | POST | Create document |
| `/api/collections/:name/:id` | GET/PUT/DELETE | Document CRUD |
| `/api/admin/stats` | GET | Admin statistics |
| `/api/admin/backup` | POST | Create backup |

### WebSocket

Realtime updates at `/realtime?token=...` (admin token required). Events: `doc.created`, `doc.updated`, `doc.deleted`, `collection.created`, `collection.deleted`.

## Important Rules

### Must Do

1. **Zero warnings** - Build must complete with zero warnings
2. **Rebuild Tailwind** - After template changes with new utility classes
3. **Restart server** - After Rust code changes: `cargo build --release && sudo systemctl restart olibuijr-rust`
4. **Run healthcheck** - After deployments: `./target/release/healthcheck`
5. **Use Shadcn components** - Never write raw UI if a Shadcn-style equivalent exists in `public/templates/components/shadcn/`
6. **Match UI styling** - Borders, backgrounds, shadows, focus rings must match existing components

### Must Not

1. **Never commit secrets** - `.env.local`, credentials, API keys
2. **Never delete reserved collections** - `users` and `settings` are core system collections
3. **Never use external crates** - Zero-dependency philosophy (except `rustls` for TLS)
4. **Never run npm install in production** - Tailwind tooling is for development only

## Database

- **Type:** In-memory document store
- **Format:** JSON-like documents with auto-generated IDs
- **Storage:** `data/db.bin` encrypted with ChaCha20-256
- **Reserved collections:** `users`, `settings` (never delete)
- **Timestamps:** Auto-generated `created_at` and `updated_at`

## Security

All cryptography is pure Rust:

| Algorithm | Standard | Purpose |
|-----------|----------|---------|
| SHA-256 | FIPS 180-4 | Hashing |
| HMAC-SHA256 | RFC 2104 | Message authentication |
| PBKDF2 | RFC 8018 | Password hashing (100k iterations) |
| ChaCha20 | RFC 8439 | Database encryption |

## URLs & Ports

- **App:** http://localhost:3460
- **Admin Panel:** http://localhost:3460/_admin
- **Documentation:** http://localhost:3460/docs
- **API:** http://localhost:3460/api/

### Production Routing

- `olibuijr.com`, `www.olibuijr.com` - Base app (public)
- `dev-$project.olibuijr.com` - Dev port for project (admin auth)
- `$project.olibuijr.com` - Prod port for project (admin auth)

## Configuration

Environment variables in `.env.local`:

```env
SECRET_KEY="your-256-bit-secret-key"  # openssl rand -hex 32
ADMIN_EMAIL="admin@example.com"
ADMIN_PASSWORD="your-secure-password"
LOG_ENABLED=1                          # Optional: 0 to disable
LOG_PATH=logs.log                      # Optional: custom log path
RPW_ROOT=/path/to/repo                 # Optional: override repo root
CORS_ORIGIN="https://example.com"      # Optional: defaults to "*" for development
```

## Code Style

1. Follow existing patterns in the codebase
2. Use Tailwind CSS classes for styling
3. Keep components modular in `public/templates/components/`
4. Validation helpers are in `src/auth.rs`, re-exported via `api/utils.rs`
5. API handlers go in `src/api/` organized by domain
6. Pages are defined in `src/pages.rs`

## Workflow Summary

```bash
# Development cycle
1. Edit code
2. cargo build --release    # Must have zero warnings
3. sudo systemctl restart olibuijr-rust
4. ./target/release/healthcheck

# Template changes with new Tailwind classes
1. Edit templates
2. node node_modules/@tailwindcss/cli/dist/index.mjs -i ./public/input.css -o ./public/styles.css --minify
3. Refresh browser (or restart if hot reload not working)
```

## Admin Panel Features

- **Dashboard** - Collection/user counts, backup creation
- **SQL Browser** - Table view of documents per collection
- **API Reference** - Auto-generated curl examples
- **Authentication Guide** - REST API token usage
- **User Management** - View registered users
- **Servers** - Nginx proxy and network details
- **Port Control** - App port and dev/prod ranges
- **Settings** - Site metadata and infrastructure defaults

---

*This file consolidates all AI agent guidance for the Rust Pure Web project.*
