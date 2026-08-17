# Security & Quality Review — IntermediAgro Users Microservice

**Document:** `docs/security-quality-review.md`
**Project:** `users_microsservice` (Rust 2021, Axum 0.7.5, SQLx 0.7.4, PostgreSQL, JWT)
**Date:** 2026-08-16
**Reviewer:** Carla (QA Engineer — adversarial code review)
**Scope:** Full security vulnerability audit, dependency CVE analysis, code quality assessment, and compliance gap review.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Risk Matrix](#2-risk-matrix)
3. [Dependency CVE Analysis](#3-dependency-cve-analysis)
4. [Secret Management](#4-secret-management)
5. [JWT Implementation Security](#5-jwt-implementation-security)
6. [SQL & Database Security](#6-sql--database-security)
7. [Input Validation & API Security](#7-input-validation--api-security)
8. [Error Handling & Panic Surfaces](#8-error-handling--panic-surfaces)
9. [Docker Security](#9-docker-security)
10. [Code Quality Issues](#10-code-quality-issues)
11. [Test Coverage Assessment](#11-test-coverage-assessment)
12. [Compliance Gaps](#12-compliance-gaps)
13. [Prioritized Remediation Plan](#13-prioritized-remediation-plan)

---

## 1. Executive Summary

The IntermediAgro Users Microservice is an **early-stage prototype** with **critical security gaps** and **zero functional authentication/authorization** as implemented. The `jwt` crate is orphaned (not wired to the binary), no input validation exists, secrets are mishandled, and multiple dependencies carry known CVEs.

**Verdict:** The service **cannot be deployed to any environment** in its current state. The security posture is inadequate even for a development deployment.

| Metric | Count |
|--------|-------|
| Critical vulnerabilities | 8 |
| High vulnerabilities | 10 |
| Medium issues | 14 |
| Low issues | 10+ |
| Tests | 0 |
| Panic sites (`unwrap`/`expect`) | ~10 |
| Known CVEs in dependencies | 4 |

---

## 2. Risk Matrix

| ID | Vulnerability | Severity | Likelihood | OWASP Category | Status |
|----|--------------|----------|------------|----------------|--------|
| S01 | SQLx 0.7.4 — RUSTSEC-2024-0363 (Postgres exploit) | **Critical** | High | A06:2021 Vulnerable Components | Confirmed |
| S02 | SECRET_KEY exposed in `.env` baked into Docker image | **Critical** | High | A02:2021 Cryptographic Failures | Confirmed |
| S03 | Default DB credentials (postgres:postgres) | **Critical** | High | A07:2021 Auth Failures | Confirmed |
| S04 | No TLS/HTTPS — traffic in plaintext | **High** | High | A02:2021 Cryptographic Failures | Confirmed |
| S05 | No CORS policy | **High** | Medium | A05:2021 Security Misconfiguration | Confirmed |
| S06 | No rate limiting — DoS exposure | **High** | Medium | A04:2021 Insecure Design | Confirmed |
| S07 | No input validation on endpoints | **High** | Medium | A03:2021 Injection | Confirmed |
| S08 | JWT crate orphaned — no authN/authZ enforced | **Critical** | Certain | A01:2021 Broken Access Control | Confirmed |
| S09 | No auth middleware — all routes unauthenticated | **High** | Certain | A01:2021 Broken Access Control | Confirmed |
| S10 | No security headers | **Medium** | High | A05:2021 Security Misconfiguration | Confirmed |
| S11 | Docker runs as root | **Medium** | High | A05:2021 Security Misconfiguration | Confirmed |
| S12 | jsonwebtoken 9.3.0 — type confusion CVE | **High** | Low | A06:2021 Vulnerable Components | Confirmed |
| S13 | ring 0.17.8 — RUSTSEC-2025-0009 | **Medium** | Low | A06:2021 Vulnerable Components | Confirmed |
| S14 | dotenv 0.15.0 — unmaintained | **Low** | Low | A06:2021 Vulnerable Components | Confirmed |
| S15 | No password hashing (token stored plaintext in DB) | **High** | High | A02:2021 Cryptographic Failures | Confirmed |
| S16 | No `sub`/`iat`/`jti`/`aud`/`iss` JWT claims | **Medium** | Medium | A01:2021 Broken Access Control | Confirmed |
| S17 | `Db` struct stores `DATABASE_URL` with credentials | **Medium** | Low | A09:2021 Logging Failures | Confirmed |

---

## 3. Dependency CVE Analysis

### 3.1 Verified CVE Table

All findings verified against RustSec and GitHub Advisory databases.

| Advisory | Package | Current Version | Severity | Remediation | Status |
|----------|---------|----------------|----------|-------------|--------|
| **RUSTSEC-2024-0363** | `sqlx` | 0.7.4 | Critical | Upgrade to `>= 0.8.1` | **VULNERABLE** — Postgres protocol exploit demonstrated |
| **CVE-2026-25537** (GHSA-h395-gr6q-cpjc) | `jsonwebtoken` | 9.3.0 | High | Upgrade to `>= 10.3.0` | Flagged — Type Confusion; partially mitigated by default `required_spec_claims` |
| **RUSTSEC-2025-0009** | `ring` (transitive) | 0.17.8 | Medium | Upgrade to `>= 0.17.12` | VULNERABLE — AES/QUIC panic under overflow checks |
| **RUSTSEC-2021-0141** | `dotenv` | 0.15.0 | Low | Replace with `dotenvy` | Unmaintained — unused in code but still a dependency |
| RUSTSEC-2020-0159 | `chrono` | 0.4.38 | N/A | Already patched | Not applicable |
| RUSTSEC-2023-0001 | `tokio` | 1.37.0 | N/A | Already patched | Not applicable |
| RUSTSEC-2022-0055 | `axum-core` | 0.4.3 | N/A | Already patched | Not applicable — default 2MB body limit present |

### 3.2 Recommended Cargo Audit

```bash
# Install cargo-audit
cargo install cargo-audit

# Run full transitive scan
cargo audit

# Add to CI pipeline
cargo audit --deny warnings
```

### 3.3 Remediation Cargo.toml

```toml
# database/Cargo.toml — fix SQLx
sqlx = { version = "0.8.1", features = ["postgres", "runtime-tokio-rustls", "migrate"] }

# jwt/Cargo.toml — fix jsonwebtoken + replace dotenv
jsonwebtoken = "10.3.0"
dotenvy = "0.15.7"  # replaces unmaintained dotenv

# Remove chrono Duration deprecation — use TimeDelta
chrono = { version = "0.4.20", features = ["clock"] }
```

---

## 4. Secret Management

### 4.1 GIT Forensics (Verified)

- `.env` is **NOT tracked** in git history (properly gitignored ✅)
- An earlier placeholder `SECRET_KEY = "mykey"` existed and was replaced with `env::var()` calls — good
- The actual `.env` file value (`<redacted>`) exists only in the working tree / Docker build context

### 4.2 Findings

| Finding | Severity | Evidence |
|---------|----------|----------|
| SECRET_KEY in `.env` baked into Docker image via `COPY . .` | **Critical** | Dockerfile line 6; no `.dockerignore` exists |
| SECRET_KEY is Base64-encoded but not rotated | **High** | `.env` line 1 — hardcoded static key, no rotation mechanism |
| DATABASE_URL contains plaintext credentials | **High** | `.env` line 3 — `postgres://<redacted>@db:5432/users` |
| No environment separation (prod/staging/dev) | **High** | Single `.env` for all environments |
| `env::var("SECRET_KEY")` called on EVERY function invocation | **Medium** | `jwt/src/lib.rs` lines 29, 46 — should be loaded once at startup |
| No secret validation (minimum length, entropy) | **Medium** | No check that SECRET_KEY meets cryptographic minimums (HS256 ≥ 256 bits) |

### 4.3 Recommendations

1. Add `.dockerignore` with `.env`, `target/`, `Cargo.lock`
2. Use Docker secrets or a vault (HashiCorp Vault, AWS Secrets Manager)
3. Load `SECRET_KEY` once at startup and pass via `Arc<str>` or application state
4. Generate keys with `openssl rand -base64 32` — minimum 256 bits for HS256
5. Rotate keys every 90 days; implement key ID in JWT header for rotation support
6. Use separate `.env.development`, `.env.staging`, `.env.production`

---

## 5. JWT Implementation Security

### 5.1 Architecture Issues

The `jwt` crate (`jwt/src/lib.rs`) implements `encode_jwt` and `decode_jwt` functions, but:

- **The `jwt` crate is NOT a dependency of the main `microsservice` binary** — it is a workspace member but absent from `[dependencies]` in `microsservice/Cargo.toml`. All JWT code is **dead code**.
- No authentication middleware exists in the Axum router
- No protected routes exist

### 5.2 Implementation Review

| Finding | Severity | File:Line | Details |
|---------|----------|-----------|---------|
| Encode/Decode type mismatch | **High** | `jwt/src/lib.rs`:31,48 | `encode_jwt` encodes `Claims { email, exp }` but `decode_jwt` decodes into `User { email }` — `exp` silently dropped |
| No `sub` claim (subject/user ID) | **High** | `jwt/src/model/claims.rs` | Claims only has `email` and `exp` — no user identifier for authorization |
| No `iat` (issued-at) claim | **Medium** | `jwt/src/model/claims.rs` | Cannot track token age or enforce max-age policies |
| No `jti` (JWT ID) claim | **Medium** | `jwt/src/model/claims.rs` | No replay protection, no token revocation list |
| No `aud` (audience) claim | **Medium** | `jwt/src/model/claims.rs` | No scope isolation between microservices |
| No `iss` (issuer) claim | **Low** | `jwt/src/model/claims.rs` | Cannot verify token origin |
| Default validation only | **High** | `jwt/src/lib.rs`:51 | `Validation::default()` only checks `exp` — no algorithm pinning, no audience, no issuer |
| No algorithm pinning | **Critical** | `jwt/src/lib.rs`:51 | Default `Validation` accepts HS256/HS384/HS512 but does not pin to a specific algorithm — vulnerable to algorithm confusion attacks |
| Deprecated `Duration::days()` | **Low** | `jwt/src/lib.rs`:33 | `chrono::Duration::days()` deprecated since 0.4.34 — use `TimeDelta::days()` |
| Non-idiomatic `return token;` | **Low** | `jwt/src/lib.rs`:42 | Should be last expression without `return` |

### 5.3 Type Mismatch Nuance (Verified)

In `jsonwebtoken` 9.x, validation runs against the raw JSON `Value` **before** deserialization into `T`. The `required_spec_claims` defaults to `{"exp"}` and validation of `exp` occurs independently of the target type. Therefore:

- **Expired tokens ARE still rejected** (exp is validated by the crate, not the struct)
- The `User` struct **silently drops** the `exp` field after decode — a **correctness/maintainability** issue, not a direct security bypass
- Downstream code using the decoded `User` has no access to token expiry metadata

### 5.4 Recommended Claims Structure

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // User ID (subject)
    pub email: String,
    pub exp: i64,         // Expiration time
    pub iat: i64,         // Issued at
    pub jti: String,      // Unique token ID (for revocation)
    pub iss: String,      // Issuer (e.g., "intermediagro-users")
    pub aud: String,      // Audience (e.g., "intermediagro-api")
    pub role: String,     // User role for authorization
}
```

### 5.5 Recommended Validation

```rust
let mut validation = Validation::new(Algorithm::HS256);
validation.set_audience(&["intermediagro-api"]);
validation.set_issuer(&["intermediagro-users"]);
validation.validate_exp = true;
validation.leeway = 60; // 60 seconds clock skew tolerance
```

---

## 6. SQL & Database Security

### 6.1 Migration Issues

The migration file `migrations/001_users_table.sql` has multiple problems:

| Line | Issue | Severity |
|------|-------|----------|
| 2 | `AUTOINCREMENT` — SQLite syntax, not PostgreSQL | **Critical** — should be `SERIAL` or `GENERATED ALWAYS AS IDENTITY` |
| 7 | `DATETIME` — not a PostgreSQL type | **Critical** — should be `TIMESTAMP` or `TIMESTAMPTZ` |
| 8 | Missing comma before `updated_at` | **Critical** — SQL syntax error, migration fails |
| 10 | Token stored as plaintext `VARCHAR(255)` | **High** — no hashing, tokens truncated if >255 chars |
| 6 | `user_type VARCHAR(255)` — no CHECK constraint | **Medium** — arbitrary values accepted |
| 1 | No `created_at`/`updated_at` auto-update trigger | **Medium** — `updated_at` will never change |
| — | No index on `email` beyond UNIQUE | **Low** — could add explicit index for query optimization |

### 6.2 Corrected PostgreSQL Migration

```sql
CREATE TABLE users (
    id          SERIAL PRIMARY KEY,
    name        VARCHAR(255) NOT NULL,
    email       VARCHAR(255) NOT NULL UNIQUE,
    password    VARCHAR(255) NOT NULL,  -- bcrypt hash
    user_type   VARCHAR(50) NOT NULL CHECK (user_type IN ('admin', 'producer', 'buyer', 'agent')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_email ON users(email);

-- Auto-update trigger for updated_at
CREATE OR REPLACE FUNCTION update_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_timestamp();
```

### 6.3 Runtime Security

| Finding | Severity | Details |
|---------|----------|---------|
| No SQL injection possible yet (no queries exist) | Info | When queries are added, use SQLx parameterized queries (already included by default) |
| DB connection pool dropped immediately | **Critical** | `main.rs` discards `connect_postgres()` result — server has no DB access |
| No migrations runner | **High** | `sqlx::migrate!()` not called; `migrate` feature not enabled |
| Superuser connection | **High** | `DATABASE_URL` uses `postgres` superuser — no least-privilege DB user |
| `Db` struct stores connection URL with credentials | **Medium** | `database/src/model/mod.rs` — leak vector if serialized or logged |

---

## 7. Input Validation & API Security

### 7.1 Current State

The service exposes a single endpoint:

```
GET / → "Hello, World!"
```

No input parameters, no body parsing, no query strings. Input validation is **vacuous** because there is no input.

However, when user endpoints are added:

| Control | Status | Recommendation |
|---------|--------|----------------|
| Input validation | ❌ Missing | Use `validator` crate or custom validation middleware |
| Request size limits | Partial | Axum 0.7 default 2MB body limit applies |
| Content-type enforcement | ❌ Missing | Add `tower-http::limit::RequestBodyLimitLayer` |
| CORS | ❌ Missing | Add `tower-http::cors::CorsLayer` with allowed origins |
| Rate limiting | ❌ Missing | Add `tower_governor` or `tower::limit::ConcurrencyLimit` |
| Security headers | ❌ Missing | Add `tower-http::set_header::SetResponseHeaderLayer` for HSTS, X-Content-Type-Options, etc. |
| Helmet-equivalent | ❌ Missing | Consider `tower-http` middleware stack |

### 7.2 Recommended Middleware Stack

```rust
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
    set_header::SetResponseHeaderLayer,
};

let app = Router::new()
    .merge(router::hello())
    .layer(RequestBodyLimitLayer::new(1024 * 1024)) // 1MB
    .layer(CorsLayer::permissive()) // tighten in production
    .layer(TraceLayer::new_for_http())
    .layer(SetResponseHeaderLayer::overriding(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    ));
```

---

## 8. Error Handling & Panic Surfaces

### 8.1 Panic Sites (All Confirmed)

Every `unwrap()` and `expect()` is a potential runtime panic that will crash the process.

| File | Line | Code | Failure Mode |
|------|------|------|--------------|
| `src/main.rs` | 24 | `env::var("DATABASE_URL").unwrap()` | Panic if env not set |
| `src/server.rs` | 40 | `.expect("Failed to bind port 8080")` | Panic if port in use |
| `src/server.rs` | 45 | `axum::serve(...).await.unwrap()` | Panic on serve error |
| `src/controller/mod.rs` | 61 | `.unwrap_or_default()` | Silent fallback (less severe) |
| `database/src/lib.rs` | 106 | `Postgres::new(url).await.unwrap()` | Panic on DB connection failure |
| `database/src/model/postgres.rs` | 139 | `PgPool::connect(...).await.unwrap()` | Panic inside a function that returns `Result` |
| `jwt/src/lib.rs` | 29 | `env::var("SECRET_KEY").expect(...)` | Panic if SECRET_KEY not set |
| `jwt/src/lib.rs` | 46 | `env::var("SECRET_KEY").expect(...)` | Panic if SECRET_KEY not set |

### 8.2 Anti-pattern: Unwrap Inside Result-Returning Function

`database/src/model/postgres.rs`:

```rust
// ANTI-PATTERN: returns Result but .unwrap() inside
impl Database<PgPool> for Postgres {
    async fn new(url: String) -> Result<Db<PgPool>, Error> {
        Ok(Db {
            pool: PgPool::connect(url.as_str()).await.unwrap(), // ← panics, never returns Err
            url,
        })
    }
}
```

Fix: use `?` operator:

```rust
impl Database<PgPool> for Postgres {
    async fn new(url: String) -> Result<Db<PgPool>, Error> {
        let pool = PgPool::connect(url.as_str()).await?;
        Ok(Db { pool, url })
    }
}
```

### 8.3 Startup() Signature Misleading

`src/server.rs` declares `pub async fn startup() -> Result<()>` but the function **can never return `Err`** — it panics on every failure path. This is semantically incorrect.

### 8.4 JWT Error Type

`jwt/src/lib.rs` returns `Result<_, String>` — non-idiomatic. Should use `thiserror` or typed errors.

---

## 9. Docker Security

### 9.1 Dockerfile Findings

| Line | Issue | Severity |
|------|-------|----------|
| 1 | `rust:1.71-slim` — EOL base image, cannot compile async traits | **Critical** (build-breaker) |
| 6 | `COPY . .` bakes `.env` (with secrets) into image layers | **Critical** (secret leak) |
| 6 | `COPY . .` also copies `target/` (bloats image, slow rebuild) | **Medium** |
| — | No `.dockerignore` exists | **Critical** |
| — | No `USER` directive — runs as root | **Medium** |
| — | No `HEALTHCHECK` instruction | **Medium** |
| — | Full Rust toolchain in runtime image (should be distroless) | **Medium** |

### 9.2 Docker Compose Findings

| Issue | Severity |
|-------|----------|
| No PostgreSQL service defined despite `DATABASE_URL=postgres://...@db:5432/...` | **Critical** |
| No volumes for persistent data | **High** |
| No health check | **Medium** |
| No resource limits | **Low** |
| No network isolation | **Low** |

### 9.3 Recommended Dockerfile

```dockerfile
FROM rust:1.80-slim AS build
WORKDIR /app
# Layer caching
COPY Cargo.toml Cargo.lock ./
COPY database/Cargo.toml database/Cargo.toml
COPY jwt/Cargo.toml jwt/Cargo.toml
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/local/bin
COPY --from=build /app/target/release/microsservice .
RUN useradd -r -s /bin/false appuser
USER appuser
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD curl -f http://localhost:8080/ || exit 1
CMD ["./microsservice"]
```

### 9.4 Recommended .dockerignore

```
.env
target/
Cargo.lock
**/*.rs.bk
*.pdb
.git/
.gitignore
docs/
```

---

## 10. Code Quality Issues

| # | Issue | File | Severity |
|---|-------|------|----------|
| Q01 | `jwt` crate is orphaned — not a dependency of the binary | `microsservice/Cargo.toml` | Critical |
| Q02 | `serde` and `serde_json` in root `Cargo.toml` but never used in root crate | `microsservice/Cargo.toml` | Low |
| Q03 | `dotenv` dependency in `jwt` crate but never called anywhere | `jwt/Cargo.toml` | Low |
| Q04 | `chrono::Duration::days()` deprecated | `jwt/src/lib.rs`:33 | Low |
| Q05 | Non-idiomatic `return token;` | `jwt/src/lib.rs`:42 | Low |
| Q06 | `Db` struct has `url` field that is never read | `database/src/model/mod.rs` | Low |
| Q07 | `Db` struct has `pool` field never used for queries | `database/src/model/mod.rs` | Low |
| Q08 | `Database` trait `new()` method over-engineered for a single backend | `database/src/model/mod.rs` | Medium |
| Q09 | `Cargo.lock` gitignored for a binary crate (should be committed) | `.gitignore` | Medium |
| Q10 | No structured error handling (no `thiserror`/`anyhow`) | Global | Medium |
| Q11 | No logging/tracing | Global | Medium |
| Q12 | `startup()` returns `Result` but can never produce `Err` | `src/server.rs` | Medium |
| Q13 | Controller returns raw `Response<String>` instead of using Axum idioms | `src/controller/mod.rs` | Low |
| Q14 | `private_interfaces` lint — `pub fn connect_postgres` returns `Db<PgPool>` whose module `model` is private | `database/src/lib.rs` | Warning |

---

## 11. Test Coverage Assessment

### 11.1 Current State

**Zero tests exist anywhere in the project.** No `#[test]`, no `#[cfg(test)]`, no `tests/` directory. Verified by reading all `.rs` files.

### 11.2 Required Test Coverage (Minimum 80%)

| Area | Tests Needed | Priority |
|------|-------------|----------|
| JWT encode/decode round-trip | 5+ unit tests | P0 |
| JWT expired token rejection | 3+ tests | P0 |
| JWT invalid signature rejection | 3+ tests | P0 |
| Controller hello endpoint | 2+ integration tests (axum-test) | P1 |
| Database connection & pool | 3+ tests (testcontainers) | P1 |
| Migration application | 2+ tests | P1 |
| User CRUD handlers (when implemented) | 15+ tests | P1 |
| Input validation | 10+ tests | P1 |
| Error handling paths | 5+ tests | P2 |
| Middleware (CORS, rate limit) | 5+ tests | P2 |

### 11.3 Recommended Test Crates

```toml
[dev-dependencies]
tokio-test = "0.4"
axum-test = "16"
testcontainers = "0.23"  # for PostgreSQL integration tests
mockall = "0.13"
pretty_assertions = "1"
```

---

## 12. Compliance Gaps

| Control | Status | Standard Reference |
|---------|--------|-------------------|
| Encryption at rest | ❌ Missing | OWASP A02:2021 |
| Encryption in transit (TLS) | ❌ Missing | OWASP A02:2021 |
| Secrets management | ❌ Missing | OWASP A02:2021 / NIST SP 800-63B |
| Authentication | ❌ Missing | OWASP A07:2021 |
| Authorization (RBAC) | ❌ Missing | OWASP A01:2021 |
| Input validation | ❌ Missing | OWASP A03:2021 |
| Logging & monitoring | ❌ Missing | OWASP A09:2021 |
| Security headers | ❌ Missing | OWASP A05:2021 |
| Dependency scanning | ❌ Missing | OWASP A06:2021 |
| Container security | ❌ Missing | CIS Docker Benchmark |
| Rate limiting | ❌ Missing | OWASP A04:2021 |
| Data protection (PII) | ❌ Missing | GDPR/LGPD |
| Audit trail | ❌ Missing | NIST SP 800-53 AU-3 |
| Incident response plan | ❌ Missing | NIST SP 800-61 |

---

## 13. Prioritized Remediation Plan

### Phase 0 — Critical Security Fixes (Immediate)

| Priority | Action | Effort |
|----------|--------|--------|
| P0 | Upgrade `sqlx` to `>= 0.8.1` (RUSTSEC-2024-0363) | 2h |
| P0 | Upgrade `jsonwebtoken` to `>= 10.3.0` | 1h |
| P0 | Add `.dockerignore` to suppress `.env` in image | 0.5h |
| P0 | Remove hardcoded `SECRET_KEY` from `.env`; use runtime injection | 2h |
| P0 | Replace default postgres credentials | 1h |
| P0 | Pin JWT validation to `Algorithm::HS256` only | 1h |

### Phase 1 — Functional Security (1-2 weeks)

| Priority | Action | Effort |
|----------|--------|--------|
| P1 | Wire `jwt` crate as dependency of `microsservice` binary | 2h |
| P1 | Implement auth middleware (Axum extractor) | 4h |
| P1 | Fix `Claims` struct with full standard claims | 2h |
| P1 | Fix encode/decode type consistency | 1h |
| P1 | Add CORS layer | 1h |
| P1 | Add rate limiting | 2h |
| P1 | Fix all `unwrap()` panic sites with proper error propagation | 4h |
| P1 | Add `.env` loading via `dotenvy` at startup | 1h |
| P1 | Implement password hashing with `argon2` or `bcrypt` | 3h |
| P1 | Fix migration SQL for PostgreSQL | 2h |

### Phase 2 — Hardening (2-4 weeks)

| Priority | Action | Effort |
|----------|--------|--------|
| P2 | Add TLS/HTTPS (reverse proxy or native `axum-server`) | 8h |
| P2 | Add security headers middleware | 2h |
| P2 | Fix Dockerfile (non-root user, health check, distroless) | 4h |
| P2 | Add PostgreSQL service to docker-compose | 2h |
| P2 | Add `cargo audit` to CI pipeline | 2h |
| P2 | Implement structured error types (`thiserror`) | 4h |
| P2 | Add `tracing`/`tracing-subscriber` logging | 4h |
| P2 | Write minimum 80% test coverage | 16h |
| P2 | Replace `dotenv` with `dotenvy` | 0.5h |
| P2 | Cache `SECRET_KEY` at startup instead of per-call `env::var` | 1h |

### Phase 3 — Compliance & Maturity (1-2 months)

| Priority | Action | Effort |
|----------|--------|--------|
| P3 | Implement RBAC with role-based middleware | 8h |
| P3 | Add token revocation list (jti tracking) | 8h |
| P3 | Add audit logging | 4h |
| P3 | Set up monitoring & alerting (Prometheus/Grafana) | 8h |
| P3 | Implement secret rotation mechanism | 8h |
| P3 | Security penetration testing | 16h |

---

## Appendix: Issue Summary by File

### `microsservice/Cargo.toml`
- Missing `jwt` dependency (orphaned crate) — **Critical**
- Unused `serde`/`serde_json` — Low

### `src/main.rs`
- Pool result discarded — **Critical**
- `env::var().unwrap()` panic — **High**

### `src/server.rs`
- `.expect()` panic — **High**
- `.unwrap()` panic — **High**
- Misleading `Result` return — Medium

### `src/controller/mod.rs`
- Raw `Response<String>` — Low

### `src/router/mod.rs`
- Only one route (`GET /`) — Info

### `src/service/mod.rs`
- No issues (trivial stub)

### `database/Cargo.toml`
- Vulnerable `sqlx` 0.7.4 — **Critical**
- `migrate` feature not enabled — **High**

### `database/src/lib.rs`
- `unwrap()` on pool — **High**
- `private_interfaces` lint — Warning

### `database/src/model/mod.rs`
- `url` field never read — Low
- `pool` field never used — Low
- Over-abstracted trait — Medium

### `database/src/model/postgres.rs`
- `unwrap()` inside `Result`-returning function — **High**
- Never returns `Err` — Medium

### `jwt/Cargo.toml`
- Vulnerable `jsonwebtoken` 9.3.0 — **High**
- Unmaintained `dotenv` — Low

### `jwt/src/lib.rs`
- Orphaned (not used by binary) — **Critical**
- Encode/decode type mismatch — **High**
- `SECRET_KEY` read per-call — Medium
- Deprecated `Duration::days()` — Low
- `return token;` — Low
- `Result<_, String>` error type — Medium
- Default `Validation` (no algo pinning) — **Critical**

### `jwt/src/model/claims.rs`
- Missing `sub`, `iat`, `jti`, `aud`, `iss` — Medium

### `jwt/src/model/user.rs`
- No issues (minimal struct)

### `migrations/001_users_table.sql`
- SQLite syntax on PostgreSQL — **Critical**
- Missing commas — **Critical**
- Plaintext token storage — **High**
- No `updated_at` trigger — Medium

### `Dockerfile`
- EOL base image (1.71) — **Critical**
- `COPY . .` ships secrets — **Critical**
- No `.dockerignore` — **Critical**
- Runs as root — Medium
- No health check — Medium

### `docker-compose.yaml`
- No PostgreSQL service — **Critical**
- No volumes — High

### `.env`
- Hardcoded `SECRET_KEY` — **High**
- Default DB credentials — **Critical**

### `.gitignore`
- `Cargo.lock` gitignored (binary crate) — Medium

---

*End of document.*
