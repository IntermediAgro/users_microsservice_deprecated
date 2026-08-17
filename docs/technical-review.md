# Technical Implementation Review — IntermediAgro Users Microservice

**Reviewer:** Tiago (Full Stack Developer, Avanade methodology)
**Date:** 2026-08-14
**Scope:** Complete source-by-source code review, dependency analysis, build/Docker/migration audit, idioms & async patterns, and compilation feasibility analysis.
**Toolchain consulted:** `context7` live docs for Axum (0.7.9 / 0.8.4), SQLx (0.7.x / 0.8.x) and `keats/jsonwebtoken` (9.x); `docs.rs` for `axum::serve` and `IntoResponse` impls.

---

## 1. Executive Summary / Verdict

| Question | Answer |
|----------|--------|
| Does the project compile with a **modern** rustc (≥ 1.75)? | ✅ Yes — but only with warnings (`private_interfaces`, deprecated `chrono::Duration`, etc.). |
| Does the project compile inside the **Docker image (`rust:1.71-slim`)**? | ❌ **NO.** Hard error `E0706`: `async fn` in traits is *not* allowed before Rust 1.75. The Docker build will fail. |
| Does `docker compose up` start a working service? | ❌ **NO.** `DATABASE_URL` points at hostname `db:5432`, but **no `db` service is defined** in `docker-compose.yaml`. `PgPool::connect` retries up to the 30s default connect timeout, then `connect_postgres(...).unwrap()` panics → container exits. |
| Does `cargo run` (locally) start a working service? | ❌ **NO** as written. `main.rs` calls `env::var("DATABASE_URL").unwrap()`, but **nothing loads `.env`** (the `dotenv` dependency lives only in the unused `jwt` crate). The binary panics unless `DATABASE_URL` is exported in the shell. |
| Is the JWT functionality reachable from the binary? | ❌ **NO.** The `jwt` workspace member is **not a dependency of the `microsservice` crate**; it is dead code never linked into the binary. |
| Are the SQL migrations correct for PostgreSQL? | ❌ **NO.** `AUTOINCREMENT`, `DATETIME`, and **missing commas** make the migration non-executable on PostgreSQL. |
| Is there any database query / users CRUD? | ❌ No. The pool is created, immediately dropped, and never used. The `Db.pool` field is read nowhere. |
| Are there tests? | ❌ Zero. No unit tests, integration tests, or `#[cfg(test)]` anywhere. |
| Does the service fulfil its stated purpose ("handle users and authentication and authorization")? | ❌ No. Only `GET /` → `"Hello, World!"` exists. |

**Bottom line:** The project is an early skeleton. It will not start in Docker, will not start locally without manual env export, and the central features (users, authN/authZ, DB access) are unimplemented. Several of the user-supplied "compilation issues" are actually *runtime* or *design* issues, but the single hard build-breaker in Docker (`async fn` in a trait under Rust 1.71) is more severe than any item on the supplied list.

---

## 2. Project Inventory

```
users_microsservice/                      (git repo root; README lives here)
└─ microsservice/                         (Cargo workspace root + root binary crate)
   ├─ Cargo.toml                          # [package] "microsservice" + inline [workspace]
   ├─ Cargo.lock                          # present on disk, but .gitignore’d
   ├─ .env / .gitignore / Dockerfile / docker-compose.yaml
   ├─ database/        (lib crate)
   │  ├─ Cargo.toml
   │  └─ src/{lib.rs, model/mod.rs, model/postgres.rs}
   ├─ jwt/             (lib crate, *orphaned — not depended on*)
   │  ├─ Cargo.toml
   │  └─ src/{lib.rs, model/mod.rs, model/claims.rs, model/user.rs}
   ├─ migrations/001_users_table.sql
   └─ src/             (binary crate)
      ├─ main.rs
      ├─ server.rs
      ├─ controller/mod.rs
      ├─ router/mod.rs
      └─ service/mod.rs
```

---

## 3. Dependency Analysis

### 3.1 Root crate `microsservice` (`Cargo.toml`)

```toml
workspace = { members = ["database", "jwt"] }   # inline-table form — see §4.1
[package]
name = "microsservice"
edition = "2021"
[dependencies]
axum = "0.7.5"
serde = { version = "1.0.199", features = ["derive"] }
serde_json = "1.0.116"
tokio = { version = "1.37.0", features = ["full"] }
database = { path = "./database/" }
```

- **`jwt` is not a dependency.** → See §C1 (Critical Bug #1). The whole JWT subsystem is never linked into the binary.
- **`serde` and `serde_json` are dead direct dependencies** of this crate. Nothing in `src/*.rs` derives `Serialize`/`Deserialize` or calls `serde_json`. They may be brought in transitively by `axum`, but as *explicit* declared deps they do nothing. (Cargo does not error on unused direct deps; only `cargo-udeps` flags them.)
- **`axum = "0.7.5"`** — 0.7.5 is *not* the latest 0.7 patch (0.7.9 is, released Nov 2024), and axum **0.8.x is the current major line** (Axum 0.8 shipped Jan 2025; `0.8.4` is current on context7). 0.8 has breaking changes — path params use `{id}` (the README example already uses `/users/{id}`), `Router::with_state` is required when state is non-empty, `Response` re-exports moved. An upgrade to 0.8 is intended for a real RFC. Staying on 0.7 is acceptable *now*; flag it as tech debt.
- **`tokio = "full"`** — convenient but pulls every feature (`process`, `signal`, `fs`, …). A microservice typically needs `["macros", "rt-multi-thread", "net", "signal"]`. Trim for smaller binaries.

### 3.2 `database` crate (`Cargo.toml`)

```toml
sqlx = { version = "0.7.4", features = ["postgres", "runtime-tokio-rustls"] }
tokio = { version = "1.37.0", features = ["full"] }
```

- **Missing `migrate` feature.** The `migrations/` directory exists, yet `sqlx::migrate!()` (the compile-time-verified migration macro) requires `features = ["postgres", "runtime-tokio-rustls", "migrate"]`. Without it, the crate *cannot* programmatically apply migrations. The app also never calls `sqlx migrate run` at runtime, so the schema is never created.
- **No `chrono` / no `uuid`** even though the migration defines `created_at`/`updated_at` and would later need row IDs (`Bearer` claim, primary key). Planned features should be declared.
- sqlx 0.7.4 is one minor behind 0.7.x patch; **sqlx 0.8.x is current** in 2026 (breaking: `PgPoolOptions`/`pool` API refinements; offline mode `.sqlx/` layout changes). Tech debt, not a blocker.
- `tokio = "full"` duplicated across crates — in a workspace it is conventional to hoist common deps to one place; not required.

### 3.3 `jwt` crate (`Cargo.toml`)

```toml
chrono = "0.4.38"
dotenv = "0.15.0"
jsonwebtoken = "9.3.0"
serde = { version = "1.0.199", features = ["derive"] }
```

- **`dotenv` is unused.** `jwt/src/lib.rs` only calls `std::env::var`. Remove `dotenv` (it is also unmaintained — `dotenvy` is the recommended successor).
- **`chrono::Duration` is deprecated** in chrono ≥ 0.4.31 in favour of `chrono::TimeDelta`. This yields a deprecation warning and will eventually be removed. *Use `TimeDelta`.*
- `jsonwebtoken = "9.3.0"` is the current 9.x — fine.
- *Important consequence of "jwt not depended on":* none of these deps are even compiled when the binary is built (`cargo build` at workspace root builds only the root package and its deps). They only matter for `cargo build -p jwt`.

### 3.4 Cross-cutting compatibility

- **`rust:1.71-slim` MSRV vs dependencies.** `axum 0.7.5` MSRV ≈ 1.66–1.75 depending on patch; `tokio 1.37` MSRV ≈ 1.66; **the decisive item is the project's own `async fn` in a trait (§5.6) which requires Rust 1.75**. So the Docker image must be `rust:1.75-bookworm` minimum; recommend a current stable (≥ `rust:1.85-bookworm-slim`).
- No `rust-version` field in any `Cargo.toml`, so MSRV is whatever the toolchain provides. Adding `rust-version = "1.75"` (or higher) is strongly advised.

---

## 4. Build / Workspace Configuration

### 4.1 The inline `[workspace]` form

`workspace = { members = ["database", "jwt"] }` at the top level is unusual but **valid TOML** — it deserialises to the same structure Cargo expects from a `[workspace]` section. Cargo accepts it and will resolve the workspace correctly. *Not a bug — a style deviation.* Conventional form is:

```toml
[workspace]
members = ["database", "jwt"]
resolver = "2"
```

Adding `resolver = "2"` is recommended for edition 2021 workspaces to get the modern feature-unification rules.

### 4.2 `Cargo.lock` is git-ignored — wrong for a binary

`.gitignore` contains:

```
 Cargo.lock
```

with an explicit comment that says *“Remove Cargo.lock from gitignore if creating an executable”* — but the project **is** an executable (`microsservice` is a binary crate). Cargo's own policy: **commit `Cargo.lock` for bins**, keep it ignored only for libraries. Current state is a reproducibility/supply-chain smell (CI builds re-resolve to latest patch versions; a future `axum 0.7.6+` MSRV bump could silently break builds). **Commit `Cargo.lock`.**

> Note: `Cargo.lock` exists *locally* (untracked) and `COPY . .` in the Dockerfile copies it into the build context — so the *Docker* build is reproducible; the issue is across developers/CI, not the Docker image.

### 4.3 No `[profile.release]` tuning / no `[patch]` / no `rust-toolchain.toml`

- No `opt-level`/`lto`/`strip` settings → release image is larger and slower than necessary. Suggest `lto = "thin"` and `strip = true` in `[profile.release]`.
- No `rust-toolchain.toml` pinning the toolchain → Docker-vs-local drift can repeat the 1.71 problem silently. Pin via:

```toml
[toolchain]
channel = "1.85"
```

### 4.4 `.env` is never loaded

The `dotenv` crate is **only** a dependency of the orphaned `jwt` crate. The `microsservice` binary has **no** dotenv mechanism, so `.env` is inert when running via `cargo run`. Environment variables only work when (a) exported in the shell, or (b) Docker Compose injects `env_file`. → Runtime Bug.

---

## 5. Module-by-Module Code Review

### 5.1 `src/main.rs`

```rust
use std::env;
use axum::http::Result;
use database::connect_postgres;
mod controller; mod router; mod server; mod service;

#[tokio::main]
async fn main() -> Result<()> {
    connect_postgres(env::var("DATABASE_URL").unwrap()).await;
    server::startup().await
}
```

**Bugs / issues:**

1. **The DB pool is created and dropped in the same statement** — `connect_postgres(...).await;` is a statement whose temporary `Db<PgPool>` is dropped at the `;`. The connection pool is destroyed immediately; the server then runs with no pool. → Logic Bug #3.
2. **`.unwrap()` on `env::var("DATABASE_URL")`** panics with no context if the var is missing (which it will, since `.env` is never loaded locally). Wrap with a proper error and log.
3. **`axum::http::Result` import is *only* there to type the return alias.** `axum::http` re-exports the `http` crate; `http::Result<()>` = `Result<(), http::Error>`. `main()` returning `http::Result` is unusual — use `anyhow::Result` or `Result<(), Box<dyn std::error::Error>>`. (It compiles because `http::Error: Error + Debug`, but it is semantically a response error type, not an app bootstrap error.)
4. **Startup order is wrong:** a flaky DB blocks boot (the server never starts even though it never uses the DB). Move DB connection behind an `Arc` shared with the router *and* make it lazy/tolerant, or remove it until actually needed.
5. **`\#[tokio::main]` with default runtime** — fine, but should be `flavor = "multi_thread"` explicitly for a server with blocking I/O later.

### 5.2 `src/server.rs`

```rust
use axum::{http::Result, Router};
use tokio::net::TcpListener;
use crate::router;

pub async fn startup() -> Result<()> {
    let routes = Router::new().merge(router::hello());
    let listener = TcpListener::bind("0.0.0.0:8080").await.expect("Failed to bind port 8080");
    println!("Listening on port 8080");
    axum::serve(listener, routes.into_make_service()).await.unwrap();
    Ok(())
}
```

- **Port `0.0.0.0:8080` hard-coded** — make configurable via `PORT` env (`std::env::var("PORT").unwrap_or("8080".into())`).
- **`axum::serve(...).await.unwrap()`** panics on shutdown error. Per current docs, `axum::serve` returns `Serve<M,S>` whose future resolves to `Result<(), _>`; propagate with `?` or at least log.
- The `Result<()>` annotation is decorative: the body ends with `Ok(())` after `.unwrap()`, so the function *cannot* return an error — every failure panics instead.
- **`Router::new().merge(router::hello())`** is a needless wrap of an already-built `Router` returned by `router::hello()`. Equivalent: just `let routes = router::hello();`.
- `println!` instead of `tracing::info!`. No request logging middleware (no `tower-http::trace::TraceLayer`).
- **No graceful shutdown** — modern axum idiom: `.with_graceful_shutdown(shutdown_signal())` triggered by SIGTERM/SIGINT. Required for clean termination in containers.

### 5.3 `src/router/mod.rs` / `src/controller/mod.rs` / `src/service/mod.rs`

```rust
// router
pub fn hello() -> Router { Router::new().route("/", get(controller::hello)) }
// controller
pub async fn hello() -> Response<String> {
    Response::builder()
        .status(StatusCode::OK)
        .body(service::hello().await)
        .unwrap_or_default()
}
// service
pub async fn hello() -> String { "Hello, World!".to_string() }
```

- **Controller uses raw `http::Response::builder()`** — works (axum-core implements `IntoResponse for http::Response<B>`; validated against current `docs.rs/axum/0.7.9`/`axum-core 0.4.5` source), but it is the *least* ergonomic handler return. Current Axum best practice is one of:
  - `async fn hello() -> String` (axum infers status 200 + content-type),
  - `(StatusCode, String)` for explicit status,
  - `impl IntoResponse` for flexibility,
  - or an `axum::Json<T>` payload.
- **`unwrap_or_default()` masks errors** — `Response::builder().body` only errors on an invalid header, but silently substituting a default response hides bugs; surface it.
- **No `GET /health` liveness endpoint.** Docker has no healthcheck (§7), and the server has no readiness probe.
- **No users/auth routes** — the entire product surface is missing. There should be e.g. `POST /users`, `POST /auth/login` → returns JWT, `GET /me` with JWT middleware, etc.
- The `service/mod.rs` is a place-holder; later the service should take a `Db`/`Arc<PgPool>` (via axum `State`), not a free function.

### 5.4 `database/src/lib.rs`

```rust
use model::{postgres::Postgres, Database, Db};
use sqlx::PgPool;
mod model;

pub async fn connect_postgres(url: String) -> Db<PgPool> {
    Postgres::new(url).await.unwrap()
}
```

- **`use model::…` appearing *before* `mod model;`** — *not a bug*. In Rust, item resolution is order-independent within a crate; the `mod` declaration just needs to exist somewhere in the module. Compiles fine.
- **`model` is private (`mod model;` not `pub mod model;`)** while `connect_postgres` is `pub` and returns `Db<PgPool>`. This triggers the **`private_interfaces` lint** (warn-by-default since Rust 1.61; was `private_in_public`/E0446 historically). The crate compiles with a warning, but external consumers *cannot name* `Db<PgPool>` (they can return it via inference only). Fix: `pub mod model;` *or* `pub use model::{Db, Database};`.
- **`.unwrap()` swallows the only error path** — same trap as §5.1. Make `connect_postgres` return `Result<Db<PgPool>, sqlx::Error>` and propagate.

### 5.5 `database/src/model/mod.rs`

```rust
use sqlx::Error;
pub mod postgres;
pub struct Db<P> { pub url: String, pub pool: P }
pub trait Database<P> { async fn new(url: String) -> Result<Db<P>, Error>; }
```

- **`Db.url` is set on construction and never read** — dead field (Logic Bug #11).
- **`Db.pool` is never queried** — no `pool.execute(...)` / `query_as(...)` anywhere (Logic Bug #12). The abstraction exists in isolation.
- **The `// pub connection: C,` commented field** is a leftover from a prior design (`Db<C, P>` with both a connection and a pool) — remove dead comments.
- **`Connection` type param `P` is unbounded (`<P>`)**; only realised as `PgPool`. The generic is premature abstraction.

### 5.6 `database/src/model/postgres.rs`  — **the Docker build-breaker**

```rust
use sqlx::{Error, PgPool};
use super::{Database, Db};
pub struct Postgres;
impl Database<PgPool> for Postgres {
    async fn new(url: String) -> Result<Db<PgPool>, Error> {
        Ok(Db {
            pool: PgPool::connect(url.as_str()).await.unwrap(),
            url,
        })
    }
}
```

- **`async fn` in a trait body** (`trait Database<P> { async fn new(...) }`). Trait-level `async fn` was **stabilised in Rust 1.75 (RFC 3185, Dec 2023)**. On Rust < 1.75 this is **E0706** (`async fn in traits are not permitted`). Since the Dockerfile pins `rust:1.71-slim`, **the Docker build fails at exactly this line**. This single incompatibility is more severe than any item on the supplied list — it is the actual compile-breaker, not merely an "old image" smell.
- **`.unwrap()` inside `new`** while the signature returns `Result<…, Error>` — the `Err` branch is *unreachable*; the function can only `Ok` or panic. The `Result` wrapper is theatrical (Logic Bug #10). Always use `?`.
- **`PgPool::connect(url)`** uses default `PgPoolOptions` (max 10 conns, 30 s acquire/connect timeout). Current best practice (validated against the context7 SQLx docs) is to construct explicitly:

```rust
PgPoolOptions::new()
    .max_connections(10)
    .acquire_timeout(Duration::from_secs(5))
    .connect(&url)
    .await?
```

- **No TLS configuration for the pool**, despite using `runtime-tokio-rustls` — fine, but the feature is currently wasted since no TLS upgrade is performed.

### 5.7 `jwt/src/lib.rs`

```rust
pub mod model;
use crate::model::{claims::Claims, user::User};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use std::env;

pub fn encode_jwt(user: User) -> Result<String, String> {
    let SECRET_KEY = env::var("SECRET_KEY").expect("SECRET_KEY must be set");
    let claims = Claims {
        email: user.email,
        exp: (Utc::now() + Duration::days(1)).timestamp(),
    };
    let token = encode(&Header::default(), &claims,
        &EncodingKey::from_secret(SECRET_KEY.as_bytes())).map_err(|e| e.to_string());
    return token;
}

pub fn decode_jwt(token: &str) -> Result<User, String> {
    let SECRET_KEY = env::var("SECRET_KEY").expect("SECRET_KEY must be set");
    let token_data = decode::<User>(token,
        &DecodingKey::from_secret(SECRET_KEY.as_bytes()),
        &Validation::default());
    match token_data {
        Ok(token_data) => Ok(token_data.claims),
        Err(e) => Err(e.to_string()),
    }
}
```

**Issues:**

1. **Encode/Decode type mismatch (Logic Bug #9).** `encode_jwt` builds `Claims { email, exp }`; `decode_jwt` decodes into `User { email }` — a *different struct* on the same wire format. Consequences:
   - `serde` ignores unknown fields by default, so decoding succeeds **but the `exp` claim is silently discarded**; the caller can never inspect expiry.
   - **`exp` is still validated by `Validation::default()`** (jsonwebtoken extracts `exp` from the *raw JSON map* before custom-claims deserialisation, per the context7 docs: `required_spec_claims` defaults to `{"exp"}` and `validate_exp = true`). So expired tokens correctly fail — but the clean design is a **single `TokenClaims` struct** used both directions, with `exp`, `iat`, `sub`, `email`.
2. **`env::var("SECRET_KEY").expect(...)`** panics at runtime if the env var is missing. Should return `Result`.
3. **`Result<String, String>`** is an anti-pattern — use a proper error enum (`thiserror`) so callers can pattern-match (e.g. `AuthError::InvalidToken` vs `AuthError::MissingSecret`).
4. **`SECRET_KEY` is an ALL-CAPS local** — works but is *unidiomatic* (Caps is for consts, not bindings).
5. **`return token;`** at the tail — redundant in Rust; just `token`.
6. **`chrono::Duration`** deprecated → `chrono::TimeDelta`.
7. **`Header::default()`** defaults to HS256 — fine, but pin explicitly `Header::new(Algorithm::HS256)` and consider `kid`/`typ`.
8. **`exp` typed `i64`** — the jsonwebtoken README recommends `usize`; either serialises to the same JSON number, but the official convention is `usize`. (Not a bug.)
9. The crate's **`use std::env`** is the only env mechanism; combined with the crate being orphaned (§C1), `SECRET_KEY` is **never actually read at runtime by the binary**.

### 5.8 `jwt/src/model/{mod.rs,claims.rs,user.rs}`

- Small structs, derive `Serialize`/`Deserialize`.
- **`User` has only `email`** — no `id`, no `user_type`, no `name`. Not aligned with the migration's user columns.
- **`Claims` has `email` + `exp`** — missing standard `sub`/`iat`/`iss`/`aud`. Per jsonwebtoken docs these are optional but recommended for proper validation (`Validation::set_audience`, `set_issuer`).
- No `#[derive(Debug, Clone)]` — both derive omitted.
- No `#[serde(deny_unknown_fields)]` — so a structurally wrong token still decodes. The mismatch in §5.7.1 is partly *because* this attribute is absent.

---

## 6. Async / Tokio Patterns

- The runtime is single-task per request (`axum::serve` → tower concurrency). Fine.
- `#[tokio::main]` defaults to multi-threaded runtime — OK.
- **Concern:** `PgPool::connect` blocks startup until a connection is established. Use `PgPoolOptions::connect_lazy` if you want the pool created without an immediate probe — appropriate when the DB may be slow to start.
- No `tokio::select!` / signal handling for graceful shutdown — see §5.2.
- No `hyper::Body` direct usage; correctly using axum's `axum::body::Body` (via `Router`).
- The async trait limitation (§5.6) actually impacts only the workspace contract, not any consumer — when you switch to a concrete `PgPool`-typed struct function, you can drop the `Database` trait entirely.

---

## 7. Docker / Compose Review

### 7.1 `Dockerfile`

```dockerfile
FROM rust:1.71-slim as build
WORKDIR /app
COPY . .
RUN cargo build --release
FROM rust:1.71-slim
WORKDIR /usr/local/bin
COPY --from=build /app/target/release/microsservice .
EXPOSE 8080
CMD ["./microsservice"]
```

| # | Issue | Severity |
|---|-------|----------|
| 7.1.1 | **`rust:1.71-slim` is too old** — `async fn` in traits needs ≥ 1.75 → **build fails (E0706)** | ❌ Critical |
| 7.1.2 | **`COPY . .` before `cargo build`** — no dependency layer cache; any `src` change **also** rebuilds every dependency from scratch (also sends entire build context, including local `target/` and `.env`) | 🔴 High |
| 7.1.3 | **No `.dockerignore`** → `.env` (containing a real-looking `SECRET_KEY`!) is **baked into the image layer**, plus `.git`, `target/`, `debug/` inflate context  | 🔴 High (secret leak) |
| 7.1.4 | **Build stage also uses `rust:1.71-slim`**; runtime uses *same* full Rust toolchain base — ~800 MB+ image. Use `debian:bookworm-slim` or `gcr.io/distroless/` as runtime base | 🟡 Med |
| 7.1.5 | **No `HEALTHCHECK`** | 🟡 Med |
| 7.1.6 | Container **runs as root** (no `USER` directive) — security: should drop privileges / use a numeric uid (e.g. `USER 1000`) | 🟡 Med |
| 7.1.7 | Single-stage build requires network access at build time — fine, but consider `cargo-chef` for reproducible cached layer caching | 🟢 Low |
| 7.1.8 | No `--release` flags tuning (`--locked`, `--offline` with a vendored registry) | 🟢 Low |

> On #7.1.2 — the supplied list flagged the *layer-caching* concern correctly, but missed that there is no `.dockerignore` so the secret `.env` literally ships inside the published image. That is a security defect, not a perf one.

### 7.2 `docker-compose.yaml`

```yaml
version: "3"
services:
  api:
    build: { context: ., dockerfile: Dockerfile }
    ports: ["8080:8080"]
    env_file: ./.env
```

| # | Issue | Severity |
|---|-------|----------|
| 7.2.1 | **No `db` service** but `DATABASE_URL=postgres://<redacted>@db:5432/users` references hostname `db` → connection times out → unwrap panic on boot → container exits (no restart policy) → service unusable | ❌ Critical |
| 7.2.2 | No `depends_on: [db]` (and no DB to depend on) | 🔴 High |
| 7.2.3 | No `restart: unless-stopped` / `on-failure` | 🟡 Med |
| 7.2.4 | No service `healthcheck:` | 🟡 Med |
| 7.2.5 | `version: "3"` is obsolete in Compose v2 (warns: *“version is obsolete”*) — remove it | 🟢 Low |
| 7.2.6 | No `volumes:` for Postgres data (moot without a DB service, but required once added) | 🔴 High once added |
| 7.2.7 | No explicit `networks:` (implicit default works, but explicit naming is good practice) | 🟢 Low |
| 7.2.8 | Hardcoded `8080:8080` mapping — make the host port configurable via `${PORT:-8080}` | 🟢 Low |

### 7.3 `.env`

```
SECRET_KEY=<redacted-256-bit-base64>
DATABASE_URL=postgres://<redacted>@db:5432/users
```

- `.env` is git-ignored ✅ (no leak via git), but the **Dockerfile bakes it into the image** because there is no `.dockerignore`.
- `SECRET_KEY` is only used by the orphaned `jwt` crate → currently irrelevant to the running binary.
- The Postgres password `postgres:postgres` is a development default — acceptable for a dev compose, never for any other env.
- **No length check**: HS256 HMAC keys should be ≥ 256 bits (32 bytes); this one is fine (length ≈ 56+). Worth a runtime assertion to fail fast.

---

## 8. Migration Analysis — `migrations/001_users_table.sql`

```sql
CREATE TABLE users(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name VARCHAR(255) NOT NULL,
  email VARCHAR(255) NOT NULL,
  token VARCHAR(255),
  user_type VARCHAR(255) NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
  UNIQUE(email)
);
```

### 8.1 Syntax / dialect correctness

| # | Item | PostgreSQL | SQLite | Verdict |
|---|------|-----------|--------|---------|
| 8.1.1 | `INTEGER PRIMARY KEY AUTOINCREMENT` | ❌ not valid syntax — PG has no `AUTOINCREMENT` keyword (errors at that token) | ✅ SQLite uses it | Use `id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY` (preferred) or `id BIGSERIAL PRIMARY KEY` |
| 8.1.2 | `DATETIME` type | ❌ not a PostgreSQL type → `ERROR: type "dat" datetime does not exist` | ✅ SQLite treats it | Use `TIMESTAMPTZ` (timezone-aware) or `TIMESTAMP` |
| 8.1.3 | **Missing comma** after `created_at DATETIME DEFAULT CURRENT_TIMESTAMP` | ❌ syntax error AS-IS | ✅ fails too | add `,` |
| 8.1.4 | **Missing comma** after `updated_at DATETIME DEFAULT CURRENT_TIMESTAMP` (before `UNIQUE(email)`) | ❌ syntax error | — | add `,` |
| 8.1.5 | Closing `);` | ✅ present | ✅ | OK |
| 8.1.6 | `VARCHAR(255)` | ✅ valid (PG coerces to `text`) — but `text` is idiomatic over length-bound varchar (no perf benefit in PG) | ✅ | idiomatic: prefer `text` |
| 8.1.7 | `UNIQUE(email)` inline | ✅ | ✅ | Consider `CONSTRAINT users_email_key UNIQUE (email)` for a named constraint |

> The user's claim #8 — *“No semicolons in proper positions”* — is **partially accurate**: the only missing items are **commas**, not semicolons; the statement terminator `);` **is** present. The real syntax breakage is #8.1.3 and #8.1.4.

### 8.2 Schema / semantic issues (will matter once migrations actually run)

- **`updated_at` never auto-updates.** PostgreSQL has no `ON UPDATE CURRENT_TIMESTAMP` (MySQL/SQLite do). Need a `BEFORE UPDATE` trigger or set it in the application layer.
- **No `CHECK (user_type IN ('...'))`** on `user_type` — looks like an enum but is unconstrained text.
- **`token VARCHAR(255)`** — nullable; consider what this column even is (JWT? hashed refresh token?); a JWT is >255 chars typically. Type is insufficient.
- **No `IF NOT EXISTS`** — re-running the migration during dev errors. (sqlx versions are idempotent per-version, not per-statement.)
- **No schema qualifier** (`public.users`) — acceptable since PG default is `public`, but explicitness is safer in multi-tenant schemas.
- **`id INTEGER` for a primary key** — PG `INTEGER` is 32-bit; once the table grows past ~2 billion rows this overflows. Prefer `BIGINT` / `GENERATED ALWAYS AS IDENTITY`.
- **No migration runner integration** — sqlx's `migrate` feature is not enabled (§3.2), so even after fixing the SQL, nothing in the app runs migrations.

### 8.3 Corrected reference SQL

```sql
CREATE TABLE IF NOT EXISTS users (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        TEXT        NOT NULL,
    email       TEXT        NOT NULL,
    token       TEXT,
    user_type   TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT users_email_key UNIQUE (email)
);

-- updated_at auto-bump (cannot be a column default in PG)
CREATE OR REPLACE FUNCTION users_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_updated_at
BEFORE UPDATE ON users
FOR EACH ROW EXECUTE FUNCTION users_set_updated_at();
```

---

## 9. Cross-Reference: Validation of the 17 user-reported bugs

> Format: `CONFIRMED` (bug is real), `REFINED` (real but needs correction), `ADDITIONAL_CONTEXT` (deeper consequence uncovered), `OVERSTATED` (less severe than implied), `REJECTED` (not actually a bug).

| # | Reported claim | Verdict | Notes |
|---|------|---------|-------|
| 1 | `jwt` in workspace but not a dep of `microsservice` — JWT code never linked | ✅ **CONFIRMED** | Dead crate. Plus its `dotenv`/`chrono`/`jsonwebtoken` deps are never compiled by `cargo build` (only by `cargo build -p jwt`). |
| 2 | `use model::…` before `mod model;`, `model` not `pub` — should still work in-crate | ✅ **REFINED** | Compiles, *but* it triggers `private_interfaces` lint (because pub `connect_postgres` returns a `Db` whose module is private). Fix: `pub use model::…;`. |
| 3 | Pool created then dropped — discarded | ✅ **CONFIRMED** | `connect_postgres(...)` is a statement whose temporary is dropped at `;` — server runs with no pool, **and** boot is hard coupled to a DB it never uses. |
| 4 | `AUTOINCREMENT` is SQLite syntax, not PG | ✅ **CONFIRMED** | Use `GENERATED ALWAYS AS IDENTITY` or `BIGSERIAL`. |
| 5 | `DATETIME` not valid in PG | ✅ **CONFIRMED** | Use `TIMESTAMPTZ`. |
| 6 | Missing comma after `created_at` | ✅ **CONFIRMED** | Hard SQL syntax error. |
| 7 | Missing comma after `updated_at` | ✅ **CONFIRMED** | Same. |
| 8 | "No semicolons in proper positions" | ✅ **REFINED** | The statement-closing `);` *is* present; the missing pieces are the two **commas** (#6,#7). So this is *partly* right: the right keyword is "commas", not "semicolons". |
| 9 | `decode_jwt` decodes to `User`, `encode_jwt` encodes `Claims` — type mismatch | ✅ **CONFIRMED + ADDITIONAL_CONTEXT** | Confirmed. The additional context I uncovered: **`exp` is still validated by `Validation::default()` (jsonwebtoken inspects the raw claims map)**, so expired tokens *do* still fail — *but* the decoded `User` silently discards `exp`, and design-wise `User`/`Claims` should be one `TokenClaims` type or properly mapped. |
| 10 | `Database::new()` returns `Result` but unwraps inside | ✅ **CONFIRMED** | The `Err` arm is unreachable; result is pure ceremony. Use `?`. |
| 11 | `Db.url` never read | ✅ **CONFIRMED** | Dead field. |
| 12 | `Db.pool` never used for queries | ✅ **CONFIRMED** | No `query_as`/`execute` anywhere. |
| 13 | Dockerfile `COPY . .` before `cargo build` — no layer caching | ✅ **CONFIRMED + ADDITIONAL_CONTEXT** | Plus: **no `.dockerignore`**, so the unsanitised `.env` (with `SECRET_KEY`) is literally copied into the published image layer — a security defect. |
| 14 | docker-compose has no postgres service but `DATABASE_URL` references `db:5432` | ✅ **CONFIRMED** | Service is non-functional on `docker compose up`. |
| 15 | Dockerfile `rust:1.71-slim` "quite old" | ✅ **CONFIRMED + ADDITIONAL_CONTEXT** | **More than old — it actively breaks the build**: `async fn` in `trait Database` requires Rust ≥ 1.75; rust 1.71 emits **E0706**. The Docker build fails *immediately* on this crate even if everything else were fixed. |
| 16 | No health check in Dockerfile or compose | ✅ **CONFIRMED** | No `HEALTHCHECK`, no compose `healthcheck:`. |
| 17 | No volume for postgres data | ✅ **REFINED** | True, but moot until a `db` service is added. Once added, you *also* need `depends_on` + `healthcheck` + `restart: unless-stopped`. |

### Additional bugs found by this review (NOT on the user's list)

| # | Bug | Severity | Where |
|----|-----|----------|-------|
| A1 | **Docker build fails E0706** — `async fn` in traits needs Rust 1.75; image is 1.71 | ❌ Critical | `Dockerfile` × `database/src/model/mod.rs` |
| A2 | **`.env` is never loaded** by the binary — `dotenv` lives only in the orphaned `jwt` crate → local `cargo run` panics on `env::var("DATABASE_URL").unwrap()` | ❌ Critical | `src/main.rs` |
| A3 | **`.env` + secret baked into Docker image** — no `.dockerignore` | 🔴 High (security) | `Dockerfile` |
| A4 | `DATABASE_URL` host `db` never resolves without a `db` service; `PgPool::connect` retries ~30 s then `unwrap()` panics → container exits | ❌ Critical | `docker-compose.yaml` + `main.rs` |
| A5 | `PRIVATE` module route triggers `private_interfaces` lint; `Db<PgPool>` exposed from pub fn while `mod model` is private | 🟡 Med | `database/src/lib.rs` |
| A6 | sqlx **`migrate` feature not enabled**; even a valid SQL migration would never be applied | 🔴 High | `database/Cargo.toml` |
| A7 | serde & serde_json are **dead dependencies** of the `microsservice` crate (only transitively used by axum) | 🟢 Low | root `Cargo.toml` |
| A8 | `dotenv` is a dead dep of `jwt`; `chrono::Duration` deprecated (`TimeDelta`) | 🟢 Low | `jwt/Cargo.toml` / `jwt/src/lib.rs` |
| A9 | `Result<String, String>` error typing; ALL-CAPS local; redundant `return` | 🟡 Med (idiom) | `jwt/src/lib.rs` |
| A10 | Port 8080 hard-coded; `println!` instead of tracing; no graceful shutdown | 🟡 Med | `src/server.rs` |
| A11 | `Router::new().merge(router::hello())` redundant wrap; low-level `http::Response::builder()` + `unwrap_or_default()` mask builder errors | 🟢 Low | `router/mod.rs`, `controller/mod.rs` |
| A12 | `startup()` claims `Result` but can never return an error (always panics on failure) | 🟡 Med | `src/server.rs` |
| A13 | `Cargo.lock` ignored *for a binary crate* — reproducibility / supply-chain risk | 🟡 Med | `.gitignore` |
| A14 | `updated_at` has no auto-update in Postgres (no trigger) and the SQL would still need a trigger | 🟡 Med | `migrations/001_users_table.sql` |
| A15 | `id INTEGER` 32-bit PK; `token VARCHAR(255)` likely too short for JWT-length tokens | 🟡 Med | migration |
| A16 | **Zero tests** anywhere (unit, integration, `#[cfg(test)]`) | 🔴 High | whole codebase |
| A17 | `Header::default()` algorithm not pinned; `Validation` uses defaults only (no audience/issuer pinning) — token forgery risk surface is wider than necessary | 🟡 Med | `jwt/src/lib.rs` |
| A18 | No `/, /health` health/readiness endpoint → ops blind spot when containerised | 🟡 Med | router |
| A19 | No `rust-toolchain.toml` pin → toolchain drift; nothing prevents re-introducing rust 1.71 | 🟡 Med | repo |
| A20 | Build stage uses `rust:1.71-slim` *and* runtime stage uses same full Rust image (~800 MB); should be `debian:bookworm-slim`/distroless runtime | 🟡 Med | `Dockerfile` |

---

## 10. Compilation Verdict (precise)

| Scenario | Result | Why |
|----------|--------|-----|
| `cargo build` on modern rustc (≥ 1.75) — root binary | ✅ Compiles with warnings | `private_interfaces`, `chrono::Duration` deprecation |
| `cargo build --workspace` on rustc ≥ 1.75 | ✅ Compiles (with same warnings + clippy noise like ` Result<String,String>` if `-W clippy`) | |
| `cargo build` inside `rust:1.71-slim` Docker image | ❌ **Fails E0706** on `trait Database<P> { async fn new(...) }` | Trait-level `async fn` requires Rust 1.75 |
| `cargo build -p jwt` on rasustc ≥ 1.75 | ✅ Compiles (deprecation warning on `chrono::Duration`) | |
| `cargo run` locally (no `DATABASE_URL` in env) | ❌ Panics at `env::var("DATABASE_URL").unwrap()` | `.env` never loaded |
| `docker compose up api` | ❌ Panics in `PgPool::connect("postgres://...@db:5432/users").await.unwrap()` after connect timeout — `db` host doesn't resolve | No `db` service defined |
| Applying `migrations/001_users_table.sql` against PostgreSQL | ❌ `AUTOINCREMENT` / `DATETIME` / missing commas → syntax error before table exists | See §8 |

---

## 11. Critical Bug Summary (priority-ranked)

1. **(P0 — Build)** Docker uses `rust:1.71-slim`; `async fn` in trait requires ≥ 1.75 → image will not build.
2. **(P0 — Boot)** `docker-compose.yaml` has no `db` service; `DATABASE_URL` → `db:5432` → unwrap panic → service unusable.
3. **(P0 — Boot)** `.env` is never loaded by the binary → local `cargo run` panics on `env::var("DATABASE_URL").unwrap()`; `dotenv` is misplaced in the orphaned `jwt` crate.
4. **(P0 — Security)** No `.dockerignore` → real-looking `SECRET_KEY` is baked into the published image.
5. **(P1 — Logic)** The connection pool is created and immediately dropped; server runs with no DB yet is hard-coupled to DB availability on boot.
6. **(P1 — Auth)** `jwt` crate is orphaned; the entire authentication/authorization subsystem the README promises is unreachable.
7. **(P1 — Migration)** The migration would not apply even if wired (no `migrate` feature) and would fail to apply (SQLite syntax, missing commas) even if enabled.
8. **(P1 — Quality)** Zero tests; displaced MSRV; ineffective error handling (`unwrap`, `String` errors); JWT encode/decode type mismatch.

---

## 12. Prioritised Remediation Plan

### Phase 1 — Make It Build & Boot (P0)
- [ ] **Dockerfile**: bump to `rust:1.85-bookworm` (or current stable) for the build stage; use `debian:bookworm-slim` for the runtime.
- [ ] **Dockerfile**: adopt `cargo-chef` (or `COPY Cargo.toml Cargo.lock ./` first → `RUN cargo build --release` for layer caching). Build with `--locked`.
- [ ] Add a **`.dockerignore`** excluding `.env`, `target/`, `.git`, `debug/`.
- [ ] **`docker-compose.yaml`**: add the `postgres` service (`image: postgres:16-alpine`, env vars, named volume `pgdata`, `healthcheck: pg_isready -U postgres`, `depends_on` with `condition: service_healthy`).
- [ ] Move `dotenvy` to the *binary* crate (`tokio` runtime, **not** `dotenv` — deprecated), and call `dotenvy::dotenv().ok()` in `main.rs` before reading env. Add `rust-toolchain.toml` pinning `1.85`.

### Phase 2 — Fix Runtime Logic (P0/P1)
- [ ] Fix `main.rs`: load env properly with `dotenvy`; share the pool via `axum::Router::with_state(AppState { db: Arc<PgPool> })`; **do not drop the pool**. Use `?`-contextual errors via `anyhow::Result`.
- [ ] Replace `connect_postgres(...).await;` with `let db = Arc::new(connect_postgres(url).await.context("db connect")?);` and `app.with_state(AppState{db})`.
- [ ] Drop the premature `Database<P>` trait; use a plain function `pub async fn connect(url: impl AsRef<str>) -> Result<PgPool, sqlx::Error>` with `PgPoolOptions`.

### Phase 3 — Fix the JWT Subsystem & Wire It In (P1)
- [ ] Add `jwt = { path = "./jwt" }` to the *binary* crate so the subsystem is reachable.
- [ ] Single `TokenClaims` struct (`sub`, `email`, `iat`, `exp`, maybe `aud`/`iss`), used for both `encode` and `decode`. Add `#[serde(deny_unknown_fields)]`.
- [ ] Return `Result<String, AuthError>` with a `thiserror` enum; no `.expect()` for SECRET_KEY (return `Err`).
- [ ] Build a `Validation` with algorithm pinned to HS256, plus `aud`/`iss` once you define them; add tests for expired/invalid tokens.
- [ ] Replace `chrono::Duration` → `chrono::TimeDelta`.

### Phase 4 — Fix the Database Layer (P1)
- [ ] Enable `migrate` feature on sqlx; add `sqlx::migrate!("./migrations").run(&pool).await?;` on startup (`with_state` state holds the pool).
- [ ] Fix the migration SQL per §8.3.
- [ ] Add the `updated_at` trigger.
- [ ] Remove unused `Db.url`.

### Phase 5 — Fix the Axum Surface (P1)
- [ ] Replace `axum::http::Result` in `main`/`startup` with `anyhow::Result`/`Result<(), Box<dyn Error>>`. Propagate `axum::serve(...).await?`.
- [ ] Add `with_graceful_shutdown` triggered on `tokio::signal::ctrl_c()`/SIGTERM.
- [ ] Replace `Response::builder()...unwrap_or_default()` with idiomatic handlers (`impl IntoResponse`, `String`, `axum::Json<T>`).
- [ ] Add `GET /health` (200 OK) so Docker healthchecks work.
- [ ] Add real endpoints — `POST /auth/login` (issues JWT), `GET /me` (JWT-validated), `POST /users`, `GET /users/{id}` — using `axum::extract::State<AppState>` and `axum::Json`.

### Phase 6 — Hygiene & Quality (P1/P2)
- [ ] Remove dead deps (`serde`/`serde_json` from root until used; `dotenv` from `jwt`).
- [ ] Commit `Cargo.lock` (binary crate).
- [ ] `[profile.release]` `lto="thin"`, `strip=true`; add `resolver = "2"` and `rust-version = "1.75"` to `Cargo.toml`.
- [ ] Add `tracing`/`tracing-subscriber` + `tower-http::TraceLayer` for structured logs.
- [ ] Add tests: unit (jwt round-trip, validation), integration (sqlx tests against a `PgPool`-bound test container), and `axum` handler tests via `axum-test`/`reqwest` against the `Router`.
- [ ] Container hardening: `USER 1000`, `HEALTHCHECK`, `--read-only` fs where possible, `restart: unless-stopped`.

---

## 13. Appendix A — Reference Patches (key files)

### 13.1 `migrations/001_users_table.sql`

```sql
CREATE TABLE IF NOT EXISTS users (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        TEXT        NOT NULL,
    email       TEXT        NOT NULL,
    token       TEXT,
    user_type   TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT users_email_key UNIQUE (email)
);

CREATE OR REPLACE FUNCTION users_touch_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END; $$;

CREATE TRIGGER users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION users_touch_updated_at();
```

### 13.2 `database/src/lib.rs` (suggested)

```rust
use anyhow::Context;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

pub async fn connect_postgres(url: &str) -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url).await.context("Falha ao conectar ao PostgreSQL")?)
}
```

### 13.3 `jwt/src/lib.rs` (suggested)

```rust
use chrono::{TimeDelta, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Serialize, Deserialize};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenClaims {
    pub sub: i64,
    pub email: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Variável SECRET_KEY não definida")]   MissingSecret,
    #[error("Token inválido: {0}")]                 InvalidToken(String),
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(e: jsonwebtoken::errors::Error) -> Self { Self::InvalidToken(e.to_string()) }
}

pub fn encode_jwt(c: &TokenClaims) -> Result<String, AuthError> {
    let secret = std::env::var("SECRET_KEY").map_err(|_| AuthError::MissingSecret)?;
    let mut h = Header::new(jsonwebtoken::Algorithm::HS256);
    h.kid = Some("intermediagro/users".to_string());
    Ok(encode(&h, c, &EncodingKey::from_secret(secret.as_bytes()))?)
}

pub fn decode_jwt(token: &str) -> Result<TokenClaims, AuthError> {
    let secret = std::env::var("SECRET_KEY").map_err(|_| AuthError::MissingSecret)?;
    let mut v = Validation::new(jsonwebtoken::Algorithm::HS256);
    // optionally: v.set_audience(&["intermediagro"]); v.set_issuer(&["users-ms"]);
    let d = decode::<TokenClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &v)?;
    Ok(d.claims)
}

pub fn token_for(email: &str, user_id: i64, ttl: TimeDelta) -> TokenClaims {
    let now = Utc::now().timestamp();
    TokenClaims { sub: user_id, email: email.to_string(), iat: now, exp: now + ttl.num_seconds().unwrap_or(86_400) }
}
```

### 13.4 `Dockerfile` (suggested, cargo-chef flavour)

```dockerfile
FROM luqven/rust-chef:latest-rust-1.85 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY --from=planner /app /app/src/. # source-only after deps cached
RUN cargo build --release --bin microsservice

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/local/bin
COPY --from=builder /app/target/release/microsservice .
RUN useradd -u 1000 -m app && chown -R app /usr/local/bin
USER 1000
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://localhost:8080/health || exit 1
ENTRYPOINT ["./microsservice"]
```

### 13.5 `docker-compose.yaml` (suggested)

```yaml
services:
  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: users
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d users"]
      interval: 5s
      timeout: 3s
      retries: 10
    restart: unless-stopped

  api:
    build: { context: ., dockerfile: Dockerfile }
    ports: ["${PORT:-8080}:8080"]
    env_file: ./.env
    depends_on:
      db: { condition: service_healthy }
    restart: unless-stopped
    healthcheck:
      test: ["CMD-SHELL", "wget -qO- http://localhost:8080/health || exit 1"]
      interval: 15s
      timeout: 3s
      retries: 5

volumes:
  pgdata:
```

### 13.6 `.dockerignore` (suggested)

```
target/
debug/
.git/
.env
.env.*
*.pdb
**/*.rs.bk
docs/
```

---

## 14. Appendix B — Library validation references

- **Axum 0.7.x → 0.8 migration pointers** (context7 `/tokio-rs/axum` v0.7.9 & v0.8.4): path params are `/users/{id}` in 0.8 (no `:`), `Router::with_state` mandatory when state is non-`()`, `axum::Json` body extraction ergonomic. README example confirms idiomatic handler pattern (`async fn root() -> &'static str` and `(StatusCode, Json<User>)`).
- **SQLx** (`/transact-rs/sqlx`): use `PgPoolOptions::new().max_connections(..).connect(...)`, `sqlx::migrate!()` macro requires the `migrate` feature, `DATABASE_URL` env for offline verification (`sqlx prepare`).
- **jsonwebtoken** (`/keats/jsonwebtoken`): `Validation` struct default sets `required_spec_claims = {"exp"}` and `validate_exp = true`; `decode::<T>` validates `exp` against the *raw* claims map (so decoding into a struct without `exp` still **expires** tokens correctly), but custom-claims deserialisation drops unknown keys unless `deny_unknown_fields` is set. Standard claims reference uses `exp: usize`.
- **axum::serve** (`docs.rs/axum/0.7.9`): signature `pub fn serve<M, S>(tcp_listener: TcpListener, make_service: M) -> Serve<M, S>`; future resolves to a `Result`, so `.await?` (or `.unwrap()`) compiles.
- **axum-core 0.4.5 IntoResponse impls** (`docs.rs`): `impl<B> IntoResponse for http::Response<B>` — accordingly, the existing `Response<String>` handler **does compile**, though it's the least ergonomic option.

---

## 15. Closing note

All severity ratings above assume the goal is a deliverable microservice (per the Avanade DoD: tests ≥ 80% coverage, clean code, secure defaults, working container). As a learning skeleton, this code is forgivable; as production-bound, substantive work remains across build, runtime, security, database, auth, and quality dimensions. The two genuinely P0 items — Docker build failure (rust 1.71 vs `async fn` in traits) and `.env` not loaded by the binary — are the ones most likely to make this project appear *completely* non-functional to whoever next runs it, and should be addressed before any further feature development.

— *Tiago, Full Stack Developer (Avanade)*
