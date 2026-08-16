# IntermediAgro — Users Microservice

![Rust](https://img.shields.io/badge/Rust-2021-orange) ![Axum](https://img.shields.io/badge/Axum-0.7.5-blue) ![SQLx](https://img.shields.io/badge/SQLx-0.7.4-green) ![Status](https://img.shields.io/badge/Status-Prototype-red)

> **IntermediAgro** — digital marketplace platform for agricultural intermediaries (agro-intermediation).
> This repository contains the **Users Microservice**: user management, authentication, and authorization.

**⚠️ Status: Prototype Scaffold** — The codebase is an early skeleton. The architecture is directionally correct, but the service currently does not implement users, authentication, or authorization. See the [Analysis Summary](#analysis-summary) below.

---

## Table of Contents

- [Overview](#overview)
- [Tech Stack](#tech-stack)
- [Project Structure](#project-structure)
- [Getting Started](#getting-started)
- [API Reference](#api-reference)
- [Analysis Summary](#analysis-summary)
- [Critical Findings](#critical-findings)
- [Documentation](#documentation)
- [Roadmap](#roadmap)
- [License](#license)

---

## Overview

IntermediAgro connects agricultural producers, buyers, and intermediaries through a digital marketplace. This repository hosts the first service — **users_microsservice** — responsible for:

- **User management** (planned): CRUD operations for users (producers, buyers, agents, admins)
- **Authentication** (planned): JWT-based login/token issuance
- **Authorization** (planned): Role-based access control (RBAC)

**Implemented today:** A single `GET /` endpoint returning `"Hello, World!"`. The JWT and database layers exist as library crates but are not yet wired to the running server.

---

## Tech Stack

| Layer | Technology | Version |
|-------|-----------|---------|
| Language | Rust | 2021 edition (Rust ≥ 1.75 required; see [Docker build issue](#critical-findings)) |
| Web framework | [Axum](https://github.com/tokio-rs/axum) | 0.7.5 |
| Async runtime | [Tokio](https://tokio.rs) | 1.37.0 |
| Serialization | [Serde](https://serde.rs) | 1.0.199 |
| Database | PostgreSQL via [SQLx](https://github.com/launchbadge/sqlx) | 0.7.4 |
| Auth | [jsonwebtoken](https://github.com/keats/jsonwebtoken) | 9.3.0 |
| Time | [chrono](https://github.com/chronotope/chrono) | 0.4.38 |
| Containerization | Docker / docker-compose | — |

---

## Project Structure

```
users_microsservice/                 ← repository root
├── README.md                        ← this file
├── README.old.md                    (original README, preserved)
├── LICENSE                          (MIT)
├── docs/                            ← analysis & review documents
│   ├── README.md                    (documentation index)
│   ├── architecture-analysis.md     (C4 model, patterns, debt, ADRs)
│   ├── technical-review.md          (code review, deps, compilation verdict)
│   └── security-quality-review.md   (CVE analysis, OWASP, remediation)
└── microsservice/                   ← Cargo workspace root
    ├── Cargo.toml                   [workspace] members: database, jwt + binary crate
    ├── Cargo.lock
    ├── .env                         (⚠ gitignored — contains SECRET_KEY & DB_URL)
    ├── .gitignore
    ├── Dockerfile                   (multi-stage, rust:1.71-slim)
    ├── docker-compose.yaml          (api service only — NO database service)
    ├── migrations/
    │   └── 001_users_table.sql      (⚠ SQLite syntax for PostgreSQL target)
    ├── src/                         ← binary crate
    │   ├── main.rs                  entry point
    │   ├── server.rs                TCP listener + axum serve (0.0.0.0:8080)
    │   ├── controller/mod.rs        hello handler
    │   ├── router/mod.rs            GET / route
    │   └── service/mod.rs           hello service
    ├── database/                    ← lib crate: SQLx PostgreSQL pool
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs               connect_postgres(url) -> Db<PgPool>
    │       └── model/
    │           ├── mod.rs           Db<P> struct + Database<P> trait
    │           └── postgres.rs      Postgres impl
    └── jwt/                         ← lib crate: JWT encode/decode (⚠ orphaned)
        ├── Cargo.toml
        └── src/
            ├── lib.rs               encode_jwt / decode_jwt
            └── model/
                ├── claims.rs        Claims { email, exp }
                └── user.rs          User { email }
```

### Layered Architecture

```
┌──────────────────────────────────────────────────────┐
│ HTTP Request                                         │
└──────────────────────┬───────────────────────────────┘
                       ▼
┌──────────────────────────────────────────────────────┐
│ Router  (src/router/mod.rs)     routes handlers      │
├──────────────────────────────────────────────────────┤
│ Controller (src/controller/mod.rs)  HTTP responses  │
├──────────────────────────────────────────────────────┤
│ Service (src/service/mod.rs)      business logic     │
├──────────────────────────────────────────────────────┤
│ Database crate  (pool, models)   │  JWT crate (auth) │
└──────────────────────────────────────────────────────┘
                       ▼
              PostgreSQL (planned service)
```

---

## Getting Started

### Prerequisites

- Rust **≥ 1.75** (async traits; the pinned `rust:1.71-slim` Docker image will **not** compile)
- PostgreSQL (or Docker)

### Local Run (as-is — expect panics)

```bash
# 1. Export required env vars (`.env` is NOT auto-loaded by the binary)
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/users"
export SECRET_KEY="$(openssl rand -base64 32)"

# 2. Run (will panic if the DB is unreachable — pool is dropped immediately anyway)
cd microsservice
cargo run
```

### Docker (currently broken — see [Critical Findings](#critical-findings))

```bash
cd microsservice
docker compose up --build
# ❌ Fails: rust:1.71 cannot compile async traits (E0706)
# ❌ Fails: no `db` service in compose while DATABASE_URL points to db:5432
```

---

## API Reference

| Method | Path | Handler | Response | Status |
|--------|------|---------|----------|--------|
| GET | `/` | `controller::hello` | `200 OK` `"Hello, World!"` | ✅ Implemented |

**Planned endpoints** (per README intent — users, authentication, authorization):

| Area | Example Endpoints | Status |
|------|-------------------|--------|
| Users | `POST /users` · `GET /users/{id}` · `PUT /users/{id}` · `DELETE /users/{id}` | ❌ Not implemented |
| Auth | `POST /auth/login` · `POST /auth/register` · `POST /auth/logout` | ❌ Not implemented |
| Authorization | Protected routes + role middleware | ❌ Not implemented |

---

## Analysis Summary

> **Consolidated from 3 parallel specialist reviews** — Architecture (Wilson), Technical (Tiago), Security/QA (Carla). Full documents in [`docs/`](./docs/README.md).

### Overall Verdict

| Claimed capability (README) | Actual state |
|-----------------------------|--------------|
| Handle users | ❌ No user routes, no CRUD, no handlers |
| Authentication | ❌ `jwt` crate exists but is **not wired into the binary** |
| Authorization | ❌ No roles, no middleware, no protected routes |
| Persistence | ❌ Pool created then immediately dropped; migrations never run |

**The skeleton is worth keeping; the wiring, migrations, infra, and hardening are all outstanding.**

---

## Critical Findings

### 🔴 P0 — Service does not run

| # | Finding | Where | Detail |
|---|---------|-------|--------|
| 1 | **Docker build fails** | `Dockerfile` | `async fn` in trait `Database<P>` requires Rust ≥ 1.75; image pins `rust:1.71-slim` → hard error `E0706` |
| 2 | **Local run fails** | `src/main.rs` | Nothing loads `.env` (`dotenv` lives only in the orphaned `jwt` crate) → `env::var("DATABASE_URL").unwrap()` panics |
| 3 | **DB unreachable in compose** | `docker-compose.yaml` | `DATABASE_URL=postgres://...@db:5432/...` but **no `db` service defined** |
| 4 | **DB pool dropped** | `src/main.rs` | `connect_postgres(...).await;` result discarded — server has zero DB access |

### 🔴 P0 — Security

| # | Finding | Where | Detail |
|---|---------|-------|--------|
| 5 | Secrets baked into Docker image | `Dockerfile:6` | `COPY . .` without `.dockerignore` ships `.env` (`SECRET_KEY`, DB credentials) into image layers |
| 6 | **RUSTSEC-2024-0363** | `database/Cargo.toml` | `sqlx 0.7.4` — critical vuln w/ demonstrated Postgres exploit; fix ≥ 0.8.1 |
| 7 | **CVE-2026-25537** | `jwt/Cargo.toml` | `jsonwebtoken 9.3.0` type-confusion; fix ≥ 10.3.0 |
| 8 | JWT algorithm not pinned | `jwt/src/lib.rs:51` | `Validation::default()` — algorithm confusion risk; pin `HS256` |
| 9 | JWT encode/decode type mismatch | `jwt/src/lib.rs` | Encodes `Claims{email,exp}` → decodes into `User{email}`; `exp` silently dropped (expiry itself still validated by crate) |
| 10 | Default DB credentials | `.env` | `postgres:postgres` superuser; no least-privilege user |

### 🟠 High — Code & Configuration

| # | Finding | Where |
|---|---------|-------|
| 11 | `jwt` crate orphaned — not a dependency of the binary (all dead code) | `microsservice/Cargo.toml` |
| 12 | Migration SQL invalid for PostgreSQL — `AUTOINCREMENT`, `DATETIME`, missing commas (fails to parse) | `migrations/001_users_table.sql` |
| 13 | Migrations never run — `sqlx` `migrate` feature not enabled, no `sqlx::migrate!()` | `database/Cargo.toml` |
| 14 | ~10 panic sites (`unwrap`/`expect`) incl. `unwrap` inside a `Result`-returning function | global |
| 15 | `Cargo.lock` gitignored for a **binary** crate — reproducibility/supply-chain risk | `.gitignore` |
| 16 | No tests anywhere (0% coverage) | global |
| 17 | No CORS, rate limiting, security headers, TLS, logging, or structured errors | global |
| 18 | Token stored **plaintext** `VARCHAR(255)` in users table; no password hashing | migration |
| 19 | RustSec: `ring 0.17.8` vulnerable (RUSTSEC-2025-0009); `dotenv 0.15.0` unmaintained | Cargo.lock |

---

## Documentation

| Document | Contents | Read first if you are… |
|----------|----------|------------------------|
| [docs/README.md](./docs/README.md) | Document index + reading order | Everyone |
| [docs/architecture-analysis.md](./docs/architecture-analysis.md) | C4 model, design patterns, dependency tree, database architecture, scalability, **24 debt items**, 6-phase roadmap, **10 ADRs** | Architect / Tech lead |
| [docs/technical-review.md](./docs/technical-review.md) | Line-by-line code review, compilation verdict (8 scenarios), migration fixes, Docker fixes, **ready-to-paste reference patches** | Developer |
| [docs/security-quality-review.md](./docs/security-quality-review.md) | CVE table with remediation versions, OWASP risk matrix (17 items), panic-site inventory, test plan to 80%, P0–P3 remediation plan | Security / QA / DevOps |

---

## Roadmap

> Detailed phases with acceptance criteria in `docs/architecture-analysis.md` §12.

| Phase | Focus | Key deliverables |
|-------|-------|------------------|
| **P0** | Make it run truthfully | Bump Rust ≥ 1.75 + Docker base; add `db` service + `.env` loading; fix & run migrations; retain pool |
| **P1** | Wire the skeleton | `jwt` as dependency; auth middleware; users CRUD; password hashing; error types |
| **P2** | Solidify | Tests ≥ 80%; structured logging; CORS/rate limits/headers; cargo-audit CI; non-root Docker |
| **P3** | Productize | RBAC; token revocation (jti); TLS; monitoring; secret rotation; pentest |
| **P4** | Microservice ecosystem | API gateway, service discovery, observability stack, event bus |
| **P5** | Scale | Read replicas, caching, horizontal scaling, circuit breakers |

**Cheapest first big win:** Fix P0 items #1–4 (build + compose + pool) and add the corrected migration + `sqlx::migrate!()` — the service then actually starts and connects to Postgres.

---

## License

[MIT](./LICENSE) — Copyright (c) 2024 IntermediAgro.

---

*Documentation consolidated by Avanade Method — 3 parallel specialist reviews (Solution Architect, Full Stack Developer, QA Engineer), cross-validated against live library docs (Context7) and security advisory databases (RustSec, GitHub).*