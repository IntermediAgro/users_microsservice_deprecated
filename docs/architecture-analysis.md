# Architecture Analysis — IntermediAgro Users Microservice

**Document:** `docs/architecture-analysis.md`
**Project:** `users_microsservice` (Rust 2021, Axum 0.7.5, SQLx 0.7.4, PostgreSQL, JWT)
**Date:** 2026-08-14
**Status:** Draft — awaiting stakeholder review
**Scope:** Full architecture review of the users microservice: structure, patterns, dependencies, database, security, deployment, scalability, debt, and roadmap.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architecture Overview](#2-architecture-overview)
3. [C4 Model](#3-c4-model)
4. [Design Patterns](#4-design-patterns)
5. [Workspace Structure](#5-workspace-structure)
6. [Dependency Analysis](#6-dependency-analysis)
7. [Database Architecture](#7-database-architecture)
8. [Security Analysis](#8-security-analysis)
9. [Deployment Architecture](#9-deployment-architecture)
10. [Scalability Assessment](#10-scalability-assessment)
11. [Architecture Debt](#11-architecture-debt)
12. [Improvement Roadmap](#12-improvement-roadmap)
13. [Key Decisions (ADR-style)](#13-key-decisions-adr-style)

---

## 1. Executive Summary

The IntermediAgro Users Microservice is **a prototype-stage scaffold**, not yet a functional service. The codebase is small (12 Rust files, ~130 lines total) and cleanly organized into a Cargo workspace with three crates (`microsservice` binary, `database` and `jwt` libraries), which is a sound division for a microservice. The **skeleton design is directionally correct** (Axum + SQLx + JWT, layered `controller → service → repository`, workspace decomposition).

However, the service **does not currently do what its README claims** ("handle users and authentication and authorization"):

| Claimed capability | Actual state |
|---|---|
| Handle users | ❌ No user routes, no CRUD, no handlers |
| Authentication | ❌ `jwt` crate exists but is **not wired into the binary** (not even a dependency) |
| Authorization | ❌ No roles, no middleware, no protected routes |
| Persistence | ❌ Connection pool is created then **immediately dropped**; migrations never run |

**Critical findings (blocking launch):**

1. **Dead DB connection** — `main.rs` calls `connect_postgres(...)` and discards the returned pool; it is dropped microseconds later. The running server has **no database access**.
2. **JWT crate orphaned** — `jwt` is a workspace member but is not in the main crate's `[dependencies]`. None of its functions are reachable from the binary.
3. **docker-compose has no PostgreSQL service** — `DATABASE_URL` points to `db:5432` but no `db` service exists. Even if the pool were retained, the app would panic on startup (`unwrap`).
4. **Migration SQL is invalid for PostgreSQL** — uses SQLite/MySQL syntax (`AUTOINCREMENT`, `DATETIME`) and is missing commas. It would fail to parse on PostgreSQL, and it is never executed by any tool.
5. **Docker build is likely broken** — the `Database` trait uses `async fn` in traits (stabilized in Rust 1.75), but the Dockerfile pins `rust:1.71-slim`. The build image cannot compile the code.
6. **Secrets committed in the working tree** — `.env` with `SECRET_KEY` and DB credentials, plus `COPY . .` in the Dockerfile bakes it **into the image**.

**Bottom line:** The architecture skeleton (workspace, layering, choice of stack) is worth preserving; the wiring, migrations, infrastructure, and security hardening are all outstanding.

---

## 2. Architecture Overview

### 2.1 Current State

A single-binary HTTP service built on the **Axum 0.7** web framework, organized as a Cargo workspace with two supporting library crates.

**Component inventory (12 Rust files):**

```
microsservice/                    # Cargo workspace root
├── Cargo.toml                    # workspace = [database, jwt]; binary deps: axum, serde, tokio, database
├── migrations/001_users_table.sql
├── Dockerfile                    # multi-stage, rust:1.71-slim
├── docker-compose.yaml           # 1 service (api only)
├── .env                          # SECRET_KEY + DATABASE_URL (plaintext, in tree)
├── src/                          # main binary crate
│   ├── main.rs                   # tokio::main; connects DB (drops pool); starts server
│   ├── server.rs                 # bind 0.0.0.0:8080; axum::serve
│   ├── controller/mod.rs         # GET / handler → builds Response<String>
│   ├── router/mod.rs             # route table: "/" → controller::hello
│   └── service/mod.rs            # hello() → "Hello, World!"
├── database/                     # library crate: PostgreSQL abstraction
│   ├── Cargo.toml                # sqlx 0.7.4 (postgres, runtime-tokio-rustls), tokio
│   └── src/
│       ├── lib.rs                # pub fn connect_postgres(url) -> Db<PgPool>
│       └── model/
│           ├── mod.rs            # Db<P> struct { url, pool }, Database<P> trait
│           └── postgres.rs       # Postgres struct + impl Database<PgPool>
└── jwt/                          # library crate: JWT encode/decode
    ├── Cargo.toml                # jsonwebtoken 9.3.0, chrono, dotenv, serde
    └── src/
        ├── lib.rs                # encode_jwt(user) / decode_jwt(token)
        └── model/
            ├── mod.rs            # pub mod claims, user
            ├── claims.rs         # Claims { email, exp }
            └── user.rs           # User { email }
```

### 2.2 What Actually Runs

At startup (`main.rs`):

1. `env::var("DATABASE_URL").unwrap()` — panics if unset.
2. `connect_postgres(url).await` — attempts `PgPool::connect`; the result is **dropped immediately** (statement end).
3. `server::startup().await` — binds `0.0.0.0:8080`, serves one route: `GET /` → `"Hello, World!"`.

The runtime behavior of the service is therefore: **an HTTP server that returns a static string, with no state, no storage, no auth, no logging, and no graceful shutdown.**

### 2.3 Design Strengths (preserve these)

- **Cargo workspace decomposition** — separating `database` and `jwt` into library crates is the correct instinct for reuse and compile-time isolation.
- **Layered module separation** — `router → controller → service` mirrors a classical layered architecture and gives future growth points.
- **Generic `Db<P>` wrapper + `Database<P>` trait** — the generic abstraction over pool type is a reasonable seed for multi-backend support (though currently only `Postgres` exists).
- **Modern, mainstream stack** — Axum + Tokio + SQLx + jsonwebtoken is a well-trodden, maintained path.

---

## 3. C4 Model

### 3.1 Level 1 — System Context

```
                        ┌───────────────────────────────────────────┐
                        │                 IntermediAgro             │
                        │              (software system)            │
                        │   "handle users and authentication and    │
                        │                    authorization"          │
                        └──────────────────┬────────────────────────┘
                                           │ HTTPS/JSON (planned)
                        ┌──────────────────▼────────────────────────┐
┌──────────────────┐    │        Users Microservice [P]             │   ┌──────────────────┐
│  Client Apps     │───▶│   Rust/Axum HTTP API — user management,   │──▶│  PostgreSQL DB  │
│ (Web/Mobile,     │    │   authn (JWT), authz                       │   │  users database  │
│  external)       │    └──────────────────┬────────────────────────┘   └──────────────────┘
└──────────────────┘                       │
                                           │ (to be defined)
                              ┌────────────▼────────────┐
                              │ Other IntermediAgro     │
                              │ microservices (planned) │
                              └─────────────────────────┘
```

- **Users Microservice** — the system under analysis. Intended to own the user lifecycle and identity concerns.
- **Client Apps** — consumers of the API (no frontend exists yet).
- **PostgreSQL** — target persistence; **not provisioned** in compose.
- **Other microservices** — the README and project name imply a microservices fleet ("intermediagro"); no contracts, gateway, or discovery exist yet.

### 3.2 Level 2 — Container

```
┌────────────────────────────────────────────────────────────────────────────┐
│                        Users Microservice                                   │
│                                                                              │
│  ┌──────────────────────┐    ┌──────────────────────┐   ┌───────────────┐   │
│  │  API Container       │    │  database crate      │   │  jwt crate    │   │
│  │  (binary, Axum/Tokio)│───▶│  (library: SQLx      │   │  (library:    │   │
│  │  routes, handlers,   │    │   PgPool wrapper)    │   │   jsonwebtoken│   │
│  │  state)              │    │                      │   │   encode/decode│  │
│  └──────────────────────┘    └──────────┬───────────┘   └───────┬───────┘   │
│                                         │                       │           │
│  ┌──────────────────────┐               │                       │           │
│  │  DB Container        │◀──────────────┘                       │           │
│  │  PostgreSQL 15/16    │  host: db:5432 (compose ref)          │           │
│  │  (NOT COMPOSED)      │                                       │           │
│  └──────────────────────┘                                       │           │
└────────────────────────────────────────────────────────────────────────────┘
         ▲
         │ HTTP :8080 (no TLS, no reverse proxy)
   ┌─────┴──────┐
   │  Clients   │
   └────────────┘
```

| Container | Tech | Status |
|---|---|---|
| API | Rust binary, Axum 0.7, Tokio 1.37 | Scaffolded; only `GET /` works |
| DB access layer | `database` crate, SQLx 0.7.4 PgPool | Written but unreachable at runtime (pool dropped) |
| JWT service | `jwt` crate, jsonwebtoken 9.3.0 | Written but **not compiled into the binary** |
| PostgreSQL | n/a | Referenced by URL only; **absent from docker-compose.yaml** |

### 3.3 Level 3 — Component (inside the API container)

```
┌───────────────────────────── API Container ─────────────────────────────┐
│                                                                          │
│  ┌────────────┐    ┌─────────────────┐    ┌────────────────────────┐    │
│  │  Router    │───▶│  Controllers     │───▶│  Services             │    │
│  │  mod::hello│    │  controller::    │    │  service::hello       │    │
│  │  GET /     │    │  hello()         │    │  (business logic)     │    │
│  └────────────┘    └─────────────────┘    └───────────┬────────────┘    │
│                                                       │ (planned)       │
│  ┌────────────────┐    ┌─────────────────────┐        │                 │
│  │ Server (axum)  │    │  middleware (none)  │        │                 │
│  │ listen :8080   │    │  auth/trace/CORS... │        │                 │
│  └────────────────┘    └─────────────────────┘   ┌────▼─────────────┐   │
│  ┌────────────────┐                               │  database crate  │   │
│  │ main (bootstrap)│                              │  Db<PgPool>      │   │
│  └────────────────┘                               │  Postgres::new() │   │
│                                                   └──────────────────┘   │
└──────────────────────────────────────────────────────────────────────────┘
```

**Gaps at the component level:** no state management (`Router` has no `with_state`), no repositories, no error handlers, no middleware stack, no request logging, no configuration module.

---

## 4. Design Patterns

### 4.1 Patterns Present

| Pattern | Where | Assessment |
|---|---|---|
| **Layered architecture** (Router → Controller → Service) | `src/router`, `src/controller`, `src/service` | Partial (only 1 trivial path); the layering exists but carries no state through it |
| **Repository-ish abstraction** | `database` crate: `Database<P>` trait + `Postgres` impl + `Db<P>` wrapper | Seed of the Repository pattern; currently the trait has only `new()` — no CRUD methods |
| **Factory / constructor** | `Database::new(url)` per backend | Good seed; switchable backends via trait impl |
| **Generic wrapper** | `Db<P>` generic over pool type | Reasonable for multi-DB, currently over-engineered for a single Postgres use case (YAGNI for now) |
| **Module-per-layer directory** | `controller/`, `router/`, `service/`, `model/` | Conventional Rust project structure ✓ |
| **Workspace decomposition** | 3 crates | Standard monorepo pattern ✓ |

### 4.2 Patterns Missing (needed for the claimed scope)

| Missing pattern | Why it matters |
|---|---|
| **State-in-Router (DI via `with_state`)** | Services need the `PgPool` and config injected; today nothing can reach the DB or JWT keys |
| **Error type / `Result` propagation** | All functions use `unwrap`/`expect`/`String` errors; no domain error enum, no `IntoResponse` error mapping → any failure panics the process or returns a bare string |
| **Auth middleware (extractor/`FromRequestParts`)** | No `Authorization: Bearer` extraction, no JWT verification per-request, no role checks → no "authorization" exists |
| **DTO / request validation layer (validator/tower-governor)** | No input validation, no `Deserialize` request structs |
| **Repository CRUD interface** | `Database` trait has no `find_by_email`, `insert_user`, etc. — the entire persistence surface is missing |
| **Observability middleware (tracing/tower-http)** | No request IDs, logs, metrics, or tracing spans; debugging in production would be blind |
| **Graceful shutdown (signal handlers)** | `axum::serve` with no `with_graceful_shutdown` — SIGTERM kills in-flight requests |
| **Config module (dotenvy + typed config struct)** | Env access scattered across crates with `.unwrap()` at point of use |
| **Migration runner (sqlx migrate or refinery)** | No mechanism executes `migrations/` at any point |

---

## 5. Workspace Structure

### 5.1 Cargo Workspace Topology

```toml
# microsservice/Cargo.toml (root)
[workspace]
members = ["database", "jwt"]

# binary crate (implicit root package)
[dependencies]
axum = "0.7.5"          # web framework
serde = 1.0.199         # serialization
serde_json = 1.0.116    # JSON (currently unused by the binary!)
tokio = 1.37.0 ("full") # async runtime
database = { path = "./database/" }   # ← only internal dep
# ❌ jwt is NOT a dependency of the binary crate
```

### 5.2 Crate Responsibilities

| Crate | Type | Purpose | Deps | Issues |
|---|---|---|---|---|
| `microsservice` | binary | HTTP API | axum, serde, serde_json, tokio, database | `serde_json` unused; `jwt` missing; `tokio/full` bloats compile |
| `database` | library | Postgres connection abstraction | sqlx 0.7.4, tokio | `default-features` not disabled → compiles MySQL + SQLite backends too; `async fn` in trait needs Rust ≥1.75 |
| `jwt` | library | JWT encode/decode | jsonwebtoken 9.3.0, chrono 0.4.38, **dotenv 0.15.0** (deprecated), serde | Orphaned (unreachable); `dotenv` unused (never called); reads env var on every call |

### 5.3 Workspace Issues

1. **Dead member** — `jwt` is in `members` but not in the binary's dependencies. `cargo build --release` compiles it (wasted CI time) and the binary never uses it. Either wire it or (until then) it is dead weight.
2. **Lockfile ignored — WRONG for a binary.** `.gitignore` excludes `Cargo.lock`. For an application/binary crate, Cargo's own guidance (quoted in the very same `.gitignore` comment) says to **commit** `Cargo.lock` for reproducible builds. Docker builds without a lockfile resolve versions at build time — nondeterministic images.
3. **`tokio = { features = ["full"] }`** in both `microsservice` and `database` — pulls the entire feature surface; prefer minimal features (`rt-multi-thread`, `macros`, `net`, `signal`, `time`).
4. **Edition and toolchain mismatch** — `edition = "2021"` is fine, but code uses `async fn` in traits (Rust ≥1.75) while the Dockerfile pins 1.71 — see §9. A `rust-toolchain.toml` should pin the version for everyone.
5. **Naming** — crate named `microsservice` (Portuguese-style) vs. domain `users_microsservice`; fine internally, but consider `intermediagro-users` for registry/repo consistency.

---

## 6. Dependency Analysis

### 6.1 Direct Dependencies (from Cargo.lock, lockfile format v3)

| Crate | Version | Purpose | Notes |
|---|---|---|---|
| axum | 0.7.5 | HTTP framework | Current stable line at the time; hyper 1.3.1 under it |
| tokio | 1.37.0 | Async runtime | `features = ["full"]` — over-inclusive |
| serde / serde_json | 1.0.199 / 1.0.116 | Serialization | OK |
| sqlx | 0.7.4 | DB toolkit | postgres + runtime-tokio-rustls enabled; default features ON |
| jsonwebtoken | 9.3.0 | JWT | Uses `ring` 0.17.8 (C crypto); reaches into RSA primitives (rsa 0.9.6) even for HS256 |
| chrono | 0.4.38 | Time | Used for `exp` claim |
| dotenv | 0.15.0 | Env loading | **Deprecated** (unmaintained since 2021; replaced by `dotenvy`) — and never even called |
| tower | 0.4.13 | Middleware layer | Transitive via axum; unused directly |

### 6.2 Dependency Tree (top-level)

```
microsservice 0.1.0
├── axum 0.7.5 ── hyper 1.3.1, tokio 1.37.0, tower 0.4.13, matchit 0.7.3, serde, ...
├── serde 1.0.199 ── serde_derive (syn 2.0.60)
├── serde_json 1.0.116
├── tokio 1.37.0 (full) ── mio, parking_lot, signal-hook-registry, ...
├── database 0.1.0 (path)
│   ├── sqlx 0.7.4
│   │   ├── sqlx-core 0.7.4 ── rustls 0.21.12 (TLS), webpki-roots
│   │   ├── sqlx-postgres 0.7.4 ── hmac, sha2, md-5, stringprep ...
│   │   ├── sqlx-mysql 0.7.4   ← ⚠ unused backend (default features)
│   │   ├── sqlx-sqlite 0.7.4  ← ⚠ unused backend → libsqlite3-sys (C compile!)
│   │   └── sqlx-macros 0.7.4
│   └── tokio 1.37.0
└── jwt 0.1.0 (orphaned — not in tree above)
    ├── jsonwebtoken 9.3.0 ── ring 0.17.8, rsa 0.9.6, simple_asn1 ...
    ├── chrono 0.4.38 ── iana-time-zone ...
    ├── dotenv 0.15.0       ← ⚠ deprecated, unused
    └── serde 1.0.199
```

### 6.3 Dependency Issues

1. **`sqlx` umbrella crate includes all backends** — Cargo.lock shows `sqlx-sqlite` and `libsqlite3-sys 0.27.0` (a C library needing `cc`) pulled into the build despite only Postgres being used. Fix: `default-features = false, features = ["postgres", "runtime-tokio-rustls"]`, or depend on `sqlx-postgres` directly. Reduces compile time and final binary size.
2. **Duplicate async stacks in lockfile** — one copy of tokio (1.37.0, unified ✓). Good.
3. **Deprecated `dotenv`** — replaced by `dotenvy`; also unused in code. Remove.
4. **`rsa`/`pem`/`simple_asn1`/`num-bigint-dig` chain** — pulled by jsonwebtoken 9.3 for RSA support; unavoidable with v9 but harmless for HS256. Acceptable.
5. **No audit tooling** — no `cargo-audit`/`cargo deny`/`cargo vet` in CI (there is no CI at all). Dependency CVEs go unnoticed.
6. **Version drift risk** — axum 0.7.5 is fine, but ecosystem (hyper 1.x, tower) was young at lock time; a `cargo update` + re-verify is due if the project resumes.
7. **`serde_json` unused in the binary** — dead dep today; will be needed as soon as real JSON endpoints exist (keep, but note).

---

## 7. Database Architecture

### 7.1 Current State

- **Driver:** SQLx 0.7.4 with `runtime-tokio-rustls` (async, TLS via rustls — good choice).
- **Connection:** `PgPool::connect(url)` — creates a **default pool** (max ~10 connections, unbounded minors). No `PgPoolOptions` (no `max_connections`, `acquire_timeout`, `min_connections` tuning).
- **Abstraction:** `Db<P> { url, pool }` wrapper; `Database<P>` trait with `async fn new(url)`. Currently `Postgres` is the only impl.
- **Migrations:** directory `migrations/001_users_table.sql` exists but **nothing executes it**. SQLx's `migrate!` macro / `sqlx migrate` CLI / runtime `Migrator` is not configured anywhere. The `database` crate does not even enable the `migrate` feature.

### 7.2 The Migration File — Invalid for PostgreSQL

```sql
CREATE TABLE users(
  id INTEGER PRIMARY KEY AUTOINCREMENT,        -- ❌ SQLite syntax
  name VARCHAR(255) NOT NULL,
  email VARCHAR(255) NOT NULL,
  token VARCHAR(255),
  user_type VARCHAR(255) NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP -- ❌ missing comma
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP -- ❌ missing comma, ❌ DATETIME is SQLite/MySQL
  UNIQUE(email)                                  -- ❌ missing comma above
);
```

| Line | Problem | PostgreSQL fix |
|---|---|---|
| `AUTOINCREMENT` | SQLite keyword; PostgreSQL has no such keyword | `id BIGSERIAL PRIMARY KEY` or `id BIGINT GENERATED ALWAYS AS IDENTITY` |
| `DATETIME` (×2) | SQLite/MySQL type; PG rejects unknown type name | `TIMESTAMPTZ NOT NULL DEFAULT now()` |
| missing comma after `created_at ...` line | syntax error | `,` |
| missing comma after `updated_at ...` line | syntax error | `,` |
| `UNIQUE(email)` | fine in PG | keep, ideally as `CONSTRAINT` + consider lowering `VARCHAR(255)` to `VARCHAR(320)` (RFC 5321 max) |
| no `CHECK` on `user_type` | free-form string | `CHECK (user_type IN ('buyer','seller','admin'))` or lookup table |
| `token` column | storing JWTs in a users table mixes authn state with identity; blocklist-style tokens belong in a separate store | drop, or keep only for refresh-token semantics with `expires_at` |

**Net effect:** the migration file is unfixable at parse time — it was copied from a SQLite tutorial. Even after fixing the syntax, nothing applies it.

### 7.3 Runtime Failure Modes

1. `connect_postgres` calls `PgPool::connect().await` which **errors if the DB is unreachable**; the error is swallowed by `unwrap()` → panic → container crashes and restarts (if restart policy existed; it doesn't).
2. Because the pool is dropped, even a successful connection closes **before the server binds**.
3. URL `postgres://<redacted>@db:5432/users` — plaintext default credentials, hostname `db` unresolvable in compose (no postgres service).

### 7.4 Database Recommendations (target)

- `PgPoolOptions::new().max_connections(N).connect_with(PgConnectOptions)` with pool sizing tied to instance vCPU.
- Enable SQLx `migrate` feature; run migrations at startup (`sqlx::migrate!().run(&pool)`) or via a separate migration job/container.
- Fix migration 001 per §7.2; add `002_…` for any auth-related columns (see roadmap) **rather than editing 001** once it ships (migrations are append-only).
- Consider `Refinery` or `sqlx migrate` CLI for team-friendly workflows.
- Add a read-replica/connection-splitting strategy only when load justifies it (see §10).

---

## 8. Security Analysis

### 8.1 JWT Flow (as designed in `jwt` crate)

```
encode_jwt(User{email})                          decode_jwt(token)
    │                                                │
    ├─ env::var("SECRET_KEY").expect(...)            ├─ env::var("SECRET_KEY").expect(...)
    ├─ Claims { email, exp: now + 1 day }            ├─ jsonwebtoken::decode::<User>(...)
    ├─ Header::default()  → HS256                    │    Validation::default()
    └─ EncodingKey::from_secret                       │    → checks signature + exp only
        └─→ token ───────────────────────────────────┴→ User { email }
```

**Issues in the flow:**

| # | Issue | Severity |
|---|---|---|
| 1 | **Secret read from env on every call** with `.expect()` → panic if unset; no caching, no validation of length | High |
| 2 | **`Validation::default()`** — no `iss`, `aud`, `sub`, or `leeway` policy; the token would be accepted by any consumer knowing the secret with any issuer | High |
| 3 | **Type mismatch encode vs decode** — encodes `Claims { email, exp }`, decodes into `User { email }`; `exp` silently discarded at decode. Two structs for one concept | Medium |
| 4 | **1-day expiry** with no refresh mechanism, no revocation/blocklist | Medium |
| 5 | **No password hashing** — the users table has no `password_hash`, no `argon2`/`bcrypt` dependency; there is no credential flow at all | Critical (for scope) |
| 6 | **`User` struct shared with domain** — JWT claims embedded in a domain model; no dedicated `AuthClaims` with `role`/`user_id` | Medium |
| 7 | **No `sub`/`user_id` claim** — only email; email changes break tokens, and email is PII in a token | High |

### 8.2 Secret Management

| Finding | Detail |
|---|---|
| `.env` committed in working tree | Contains `SECRET_KEY=<redacted>...` and DB credentials. The project **is not a git repo yet** (`Is directory a git repo: no`), but the file exists on disk and `COPY . .` in Docker would bake it into image layers |
| **No .dockerignore** | `COPY . .` copies `.env`, `target/` (GBs), and `Cargo.lock` into the build context and image |
| No staged secrets | No Vault/Key Vault/`{env:VAR}` injection, no compose `secrets:` section, no rotation policy |
| Default Postgres creds | `postgres:postgres` — must be overridden per environment |
| Commented-out `env!("SECRET_KEY")` | Compile-time env; not usable at runtime — the commented code hints at a mismatched design between compile-time and runtime config |

### 8.3 Transport & Perimeter

- **No TLS** — plain HTTP on `:8080`; in production a reverse proxy (nginx/Caddy/APIM) or axum-server rustls must terminate TLS.
- **No CORS** — fine for now (same-origin unknown), but will be needed for web clients.
- **No rate limiting** — login/register endpoints (future) without `tower-governor` are brute-force targets.
- **Headers** — no security headers middleware (works for API, mostly n/a), no `tower-http` `SetResponseHeaderLayer`.

### 8.4 What "Security by Design" Requires Next (see roadmap)

Admin roles distinctly modelable in `user_type`; password hashing with argon2id + per-user salt; claim validation with `Validation::set_issuer/audience`; short-lived access tokens (15–30 min) + refresh tokens (7 days) with rotation; secret rotation via env injection at deploy; SQLx uses **prepared statements + bound parameters** (never string interpolation — SQLx already prevents injection when used properly).

---

## 9. Deployment Architecture

### 9.1 Dockerfile — Current

```dockerfile
FROM rust:1.71-slim as build      # ① old toolchain
WORKDIR /app
COPY . .                          # ② everything, incl. .env, target/, .git
RUN cargo build --release         # ③ cold build every time (no dep caching)

FROM rust:1.71-slim               # ④ full Rust image as runtime (~700 MB+)
WORKDIR /usr/local/bin
COPY --from=build /app/target/release/microsservice .
EXPOSE 8080
CMD ["./microsservice"]
```

**Confirmed issues:**

1. **Toolchain too old (Critical)** — `async fn` in traits requires Rust ≥ 1.75 (stabilized Dec 2023); image is `rust:1.71`. The `database` crate's `Database` trait will **not compile** in this image. This alone breaks `docker compose build`.
2. **No layer caching** — `COPY . .` before `cargo build` invalidates every layer on any change (including `.env` edits). Best practice: `COPY Cargo.toml Cargo.lock` → `cargo build --release` with dummy src (or `cargo fetch`) → then `COPY src/` → rebuild. 
3. **No .dockerignore** — build context carries `.env` (secret!), `target/` (tens of GB after a host build), `.git`. 
4. **Runtime image is a Rust toolchain image** — the second stage keeps ~700 MB–1 GB of compilers and cargo. Use `debian:bookworm-slim` (+ `ca-certificates`, `libssl`/`libpq` as needed) or `gcr.io/distroless/cc` for a 10–20× smaller, non-shell runtime.
5. **Binary not installable in a slim image without libs** — sqlx with rustls needs no libpq (pure rust TLS ✓), but `ring` requires `libgcc`/`libc` — a slim Debian base covers it; distroless/cc definitely.
6. **No healthcheck metadata** — compose/Orchestrator cannot liveness-probe the container.
7. **No non-root user** — container runs as root; add `USER` and drop capabilities.

### 9.2 docker-compose.yaml — Current

```yaml
version: "3"                 # deprecated in modern Compose
services:
  api:
    build: { context: ., dockerfile: Dockerfile }
    ports: ["8080:8080"]
    env_file: ./.env
```

**Gaps:**
- ❌ **No `db` (PostgreSQL) service** — while `DATABASE_URL` targets host `db:5432`. Compose `up` produces an API that panics on boot.
- ❌ No `depends_on` / `healthcheck` / `condition: service_healthy` ordering.
- ❌ No `restart` policy, no `volumes` (named volume for PG data — data loss on `down -v`… actually on any recreate without volume).
- ❌ No network isolation (single default network; DB exposed only internally should be on a private network with the API on the ingress network).
- ❌ No environment separation (`dev`/`staging`/`prod` overrides); `.env` is for compose variable substitution AND app secrets — conflated.
- ❌ No migration runner service (one-shot `migrate` container).

### 9.3 Target Deployment (see roadmap)

```
compose ──▶ api (rust binary, non-root)
              │  :8080
              ▼
           postgres:16-alpine (named volume, healthcheck)
              ▲
              │ one-shot migrate job (sqlx migrate)
   [prod] ingress: reverse proxy + TLS (Caddy/nginx/APIM)
   [prod] secrets via {env:VAR} injection (no .env in image)
```

---

## 10. Scalability Assessment

### 10.1 Current Capacity

With `GET /` returning a static string and no DB touch, the service can trivially saturate a single core with thousands of RPS — but it does **nothing useful**, so throughput is meaningless. It is a prototype: **"Can it scale?" is the wrong question today; "Can it *do anything*?" is the right one.** Once real endpoints exist:

| Dimension | Current | Required for scale |
|---|---|---|
| Concurrency | Tokio multi-thread (default: cores) | ✓ already correct by design |
| DB connection pool | Default PgPool (max 10) — **dropped** | `PgPoolOptions` tuned to cores; pool must be owned by app state |
| Statelessness | Stateless today (no state at all) | ✓ JWT auth is horizontally scalable *if* secret is shared/rotation-safe and no in-memory state added |
| Read throughput | n/a | Add indexes on `email` (UNIQUE already), `user_type` if filtered |
| Write throughput | n/a | Single-writer PG is fine to ~tens of k TPS; beyond that: partitioning, then read replicas (`sqlx` supports read/write splitting manually), then CQRS |
| Cache | none | Consider Redis/ValKey for sessions/token blocklist when needed; do **not** add it preemptively |
| Observability | none | **Blocking for scale**: no metrics (Prometheus), no tracing (OTel), no logs → cannot measure, cannot autoscale |
| Graceful shutdown | none | Required for zero-downtime deploys |
| Health/readiness | none | Required for LB draining + orchestrator probes |

### 10.2 Horizontal Scaling Prerequisites (checklist)

1. Stateless app instances (JWT validation is stateless — good) behind a load balancer.
2. DB pooling sized per instance: `max_connections ≈ (vCPU × 4)` with a hard cap far below PG `max_connections` (100 default).
3. Automated migrations that are idempotent-safe (sqlx MIGRATOR tracks applied versions in `_sqlx_migrations` ✓).
4. Service discovery / DNS: compose `db` hostname works for swarm/K8s via DNS; for K8s, use a `Service` + `ConfigMap`/`Secret`.
5. Observability pipeline (metrics + logs + traces) — non-negotiable before autoscaling.
6. Env-based config with 12-factor discipline (config injectable, not baked).

### 10.3 When to Add More

- **Recommended now:** — pooling config, graceful shutdown, health endpoint, observability stubs.
- **At ~100 concurrent users / first real traffic:** rate limiting, refresh-token rotation.
- **At ~1k+ RPS on writers:** read replicas; PG partitioning for `users` if >10M rows.
- **Do not add:** caching layers, message queues, event bus, Kubernetes — until there are two services and actual load. (Simplicity wins; see §13.)

---

## 11. Architecture Debt

Ranked by severity. Legend: **C** = Critical (blocks launch/security), **H** = High (serious defect or major missing capability), **M** = Medium (quality/design), **L** = Low (polish).

| # | Item | Severity | Evidence |
|---|---|---|---|
| 1 | **DB pool created and immediately dropped** — server runs with zero DB access | **C** | `main.rs:17` — `connect_postgres(...).await;` result unused |
| 2 | **JWT crate orphaned** — not in binary `Cargo.toml`; auth code unreachable | **C** | `Cargo.toml` (root) deps list has no `jwt`; lockfile confirms |
| 3 | **docker-compose lacks `db` service** — runtime panic on boot (`db` unresolved) | **C** | `docker-compose.yaml` (1 service), `.env` URL host `db` |
| 4 | **Migration SQL invalid & never executed** — SQLite syntax + missing commas; no runner | **C** | `migrations/001_users_table.sql:2,9,10` |
| 5 | **Docker build impossible on pinned toolchain** — async fn in traits needs ≥1.75; image is 1.71 | **C** | `database/model/mod.rs:12`, `Dockerfile:1` |
| 6 | **Secret in build context & image** — `.env` copied by `COPY . .`; no `.dockerignore` | **C** | `Dockerfile:5`, tree contains `.env` |
| 7 | **No error handling** — `.unwrap()/.expect()` on env, DB, serve; panics on any failure | **H** | `main.rs:17`, `database/lib.rs:6`, `postgres.rs:10`, `server.rs:11,16`, `jwt/lib.rs:12,29` |
| 8 | **No authentication or authorization surface** — no middleware, no protected routes, no role checks | **H** | `router/mod.rs` — single public route |
| 9 | **No tests** (unit, integration, or contract) | **H** | no `#[cfg(test)]` anywhere |
| 10 | **No graceful shutdown / health endpoint** | **H** | `server.rs` — `axum::serve` without `with_graceful_shutdown` |
| 11 | **JWT design flaws** — env-secret per call; decode into wrong struct; no iss/aud; 24h expiry without refresh | **H** | `jwt/lib.rs` |
| 12 | **No password hashing** — table has no password column; authn scope unmet | **H** | migration file; crate list |
| 13 | **`Cargo.lock` gitignored for a binary crate** — non-reproducible builds | **M** | `.gitignore:10` (comment even says so) |
| 14 | **sqlx default features** — compiles MySQL/SQLite + libsqlite3 C code | **M** | `database/Cargo.toml:10`, lockfile `sqlx-sqlite`, `libsqlite3-sys` |
| 15 | **Deprecated `dotenv` dependency** (unused) | **M** | `jwt/Cargo.toml:10` |
| 16 | **`Database<P>` generic abstraction (YAGNI)** — one backend, generic trait, private types leaking (`mod model` not `pub`) | **M** | `database/src/lib.rs:3` — `mod model;` private while `connect_postgres` is `pub` (private-in-public lint) |
| 17 | **Static `Response<String>` instead of `impl IntoResponse`** — bypasses Axum's response machinery | **M** | `controller/mod.rs:5` |
| 18 | **`axum::http::Result` as main return type** — wrong semantic (http::Error, not io/app) | **L** | `main.rs:3,14`, `server.rs:1,6` |
| 19 | **`tokio` "full" feature** in two crates | **L** | both Cargo.toml |
| 20 | **Container runs as root; no healthcheck; runtime image = full Rust toolchain (~1 GB)** | **M** | `Dockerfile:9` |
| 21 | **README is a stub** (2 lines) — no run instructions, no API contract | **L** | `README.md` |
| 22 | **`token` column in users table** — semantics undefined; JWT blocklist in a users table is an anti-pattern | **M** | migration file |
| 23 | **No config module** — env vars read ad hoc everywhere; commented-out `env!("SECRET_KEY")` shows config confusion | **M** | `main.rs:11`, `jwt/lib.rs:9` |
| 24 | **No observability** — `println!` for startup, no tracing/metrics | **H** | `server.rs:13` |

**Debt summary:** 6 critical, 7 high, 9 medium, 3 low. The critical debt makes the *shipped* artifact non-functional; the high debt blocks any attempt to meet the README's stated purpose.

---

## 12. Improvement Roadmap

Phased, with acceptance criteria per phase. Each phase is independently shippable.

### Phase 0 — Make it run truthfully (1–2 days)

1. **Fix floor issues:**
   - Wire `jwt` into the binary (`jwt = { path = "./jwt/" }`).
   - Retain the pool: `let db = connect_postgres(...).await;` and store it in `Router` state (`with_state`).
   - Add `db` service to compose (postgres:16-alpine, named volume, healthcheck, `depends_on: api: condition: service_healthy`).
   - Rewrite `001_users_table.sql` for PostgreSQL (BIGSERIAL identity, TIMESTAMPTZ, commas) — or keep `001` as the *documented failed draft* and create `002_users_table.sql` clean, since shipping a broken 001 sets no precedent; simplest: fix in place (not yet deployed anywhere).
2. **Make build reproducible:** add `.dockerignore` (`.env`, `target/`, `.git`), commit `Cargo.lock`, add `rust-toolchain.toml` (e.g., `channel = "1.75"` or a current stable), bump Dockerfile `FROM rust:1.75-slim` (or later stable).
3. **Graceful shutdown + health:** add `/healthz` (readiness: pool ping) and `/readyz`; `axum::serve` + `with_graceful_shutdown(SIGTERM/SIGINT)`.
4. **Acceptance:** `docker compose up` starts API + DB; `GET /healthz` → 200; migrations applied; `GET /` → 200.

### Phase 1 — Error handling & config foundations (2–3 days)

1. Introduce a crate-local `error.rs` with `AppError` enum (thiserror) implementing `IntoResponse` (status + JSON body).
2. Replace every `unwrap`/`expect` in request paths with `?`.
3. Config module: `dotenvy` + typed `Config { database_url, jwt_secret, port, jwt_ttl }` loaded once at startup and injected via state (secret passed explicitly, never `env::var` at call sites).
4. `PgPoolOptions` with explicit sizing; pool in `AppState` (Arc).
5. Add `tracing` + `tracing-subscriber` (JSON in prod); replace `println!`.
6. **Acceptance:** `cargo clippy -- -D warnings` clean; failures return structured 4xx/5xx; unit tests for error mapping.

### Phase 2 — Real auth (3–5 days)

1. **Password hashing:** add `argon2` crate; `users` gets `password_hash`, `created_at/updated_at` (proper PG types).
2. **Endpoints:** `POST /auth/register`, `POST /auth/login`, `POST /auth/refresh`, `GET /users/me`, (admin) `GET /users` — with validation (`validator` or hand-rolled), DB constraints enforced (UNIQUE email).
3. **JWT hardening:**
   - `Claims { sub (user_id), email, role, iat, exp, iss, aud }`; single struct for encode & decode.
   - `Validation` with explicit issuer/audience; access token 15–30 min; refresh token (opaque or JWT) 7 days with rotation + blocklist table.
   - Load secret once into config; `#[derive(Clone)]` state carries it.
4. **Auth middleware:** `FromRequestParts` extractor `AuthUser` verifying Bearer token; `user_type`-based `admin` guard (authorization).
5. **Security:** rate limiting (`tower-governor`) on auth endpoints; require TLS at edge.
6. **Acceptance:** register→login→/users/me round-trip; expired/replayed/forged tokens rejected (integration tests).

### Phase 3 — Observability & hardening (2–3 days)

1. Tracing spans per request (tower-http `TraceLayer`); request ID propagation.
2. Prometheus metrics (`metrics`/`axum-prometheus`): request count/latency/DB pool stats.
3. SQLx `migrate!` at startup with `Migrator` (or `sqlx migrate run` in a one-shot compose service).
4. Docker: slim runtime (`debian:bookworm-slim` or distroless `cc`), non-root `USER`, healthchecks; `cargo-audit` in CI.
5. **Acceptance:** `/metrics` scrapes; simulated crash restarts cleanly; `docker compose up` from clean clone works with zero host Rust toolchain.

### Phase 4 — Platform readiness (as needed, ~1 week)

1. CI/CD (GitHub Actions): fmt → clippy → test → `cargo audit` → build/push image → deploy (compose or AKS).
2. Secrets: injected via platform secrets (GitHub Secrets / Azure Key Vault / compose `secrets:`), never files in the tree; rotation policy (90 days).
3. If multi-service: gateway (e.g., BFF/Azure APIM managed by the broader IntermediAgro platform), central oidc discovery, per-service DBs.
4. Contract testing (e.g., Pact or OpenAPI + `utoipa` docs generation).
5. **Acceptance:** deploy pipeline green; secrets rotated with zero downtime; `cargo deny` audit clean.

### Phase 5 — Scale levers (only under real load)

Read-replica splitting → cache layer (user profiles) → partitioning → per-tenant sharding. Quantified thresholds in §10.3.

---

## 13. Key Decisions (ADR-style)

| ADR | Decision | Rationale | Trade-offs |
|---|---|---|---|
| **A-001** | Keep Axum 0.7 + Tokio + SQLx (postgres, rustls) | Mainstream, async-native, type-safe SQL; Rust ecosystem default | Axum <0.8 has no built-in OpenAPI; tower ecosystem maturity was early — re-check on resume |
| **A-002** | Keep 3-crate workspace (binary + database + jwt) | Independent compile boundaries; jwt may later be shared with other services; testability | Overhead of workspace is trivial at this size; avoid adding more crates until real need |
| **A-003** | Wire pool into `Router` state (DI) — no globals | Axum's recommended pattern; testable handlers | Requires threading state through every handler signature |
| **A-004** | `thiserror` domain errors + `IntoResponse` | Idiomatic axum; maps DB/JWT errors to status codes | Slightly more boilerplate than `anyhow`; use `anyhow` only in `main` bootstrap |
| **A-005** | Move to Rust ≥1.75 + `rust-toolchain.toml` | Lockfile-consistent, enables async traits; Docker must match | Older environments unable to build (explicit, documented) |
| **A-006** | `dotenvy` + typed config loaded once | 12-factor; eliminates env-var-at-call-site panics; secrets injected at deploy | All env plumbing centralized in one bootstrap module |
| **A-007** | Migration strategy: SQLx `migrate!` at startup (+ one-shot job in prod) | Zero extra tooling; versioned, ordered, recorded in `_sqlx_migrations` | App startup fails fast if migrations pending — acceptable for users service (by design) |
| **A-008** | Access (15–30 min) + refresh (7 d, rotating) tokens | Minimizes stolen-token window; enables revocation of refresh | Requires refresh endpoint + blocklist/rotation table (more state, more code) |
| **A-009** | Non-root, distroless/slim runtime image with healthchecks | Attack surface + image size reduction; rootless = defense in depth | Slight debugging inconvenience; `exec` shells absent — mitigate via structured logs |
| **A-010** | Defer K8s/cache/event-bus until ≥2 services + real load | Avoid over-engineering; compose suffices for one service | Re-platforming cost later is bounded by keeping config externalized (12-factor) |

---

## Appendix A — Validated Claims from Prior Review

All eight issues you identified were **confirmed by source inspection**, with two corrections/refinements:

1. ✅ **No postgres in compose** — confirmed; even a retained pool would fail (`db` unresolvable).
2. ✅ **Migration uses SQLite syntax** — confirmed (`AUTOINCREMENT`, `DATETIME`); additionally the file is **never executed** (no runner configured) — the issue is worse than stated.
3. ✅ **Missing commas** — confirmed on lines 9–10.
4. ✅ **jwt not in main Cargo.toml** — confirmed; `jwt` is compiled but orphaned.
5. ✅ **Pool discarded** — confirmed at `main.rs:17`; worse: the pool is **dropped at end of statement**, so the server has no connections at all.
6. ✅ **No tests / no error types / unwraps** — confirmed; catalogued in §11.
7. ✅ **No layer caching in Dockerfile** — confirmed; *additional finding of the same class*: `COPY . .` also ships `.env` (secret) and `target/` into the image.
8. 🔺 **Bonus critical finding:** Docker image `rust:1.71-slim` **cannot compile** the code (`async fn` in traits requires ≥1.75). The entire "deploy" path is broken before it starts.

## Appendix B — Metrics

- Lines of Rust: ~130 across 12 files
- Workspace crates: 3 (1 binary, 2 libraries)
- Routes: 1 (`GET /`)
- Tests: 0
- `unwrap`/`expect`/panic sites: ~10
- Direct dependencies: 23 first-party+third-party (8 declared)
- Transitive packages in lockfile: ~130
- Critical/High/Medium/Low debt items: 6 / 7 / 9 / 3

---

*Prepared by Wilson (Solution Architect) — await stakeholder review before implementation begins. Suggested review order: §11 debt table → §12 Phase 0 → §13 ADR A-003/A-005/A-007 approvals.*