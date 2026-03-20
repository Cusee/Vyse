# Contributing to Vyse

> **Built with effort. Broken by patterns. Protected by Vyse.**

First off — thank you. Vyse is a security tool protecting real ML APIs from real attackers, and every contribution matters. This document is the single source of truth for contributing code, documentation, tests, plugins, or ideas.

Please read it fully before opening your first issue or PR. It will save everyone time.

---

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [The Architecture — Know What You're Touching](#the-architecture)
3. [Ways to Contribute](#ways-to-contribute)
4. [Security Vulnerability Disclosure](#security-vulnerability-disclosure)
5. [Getting Started — Development Environment](#getting-started)
6. [Working with gRPC & Protobuf](#working-with-grpc--protobuf)
7. [Branching & Commit Conventions](#branching--commit-conventions)
8. [Pull Request Process](#pull-request-process)
9. [Testing Requirements](#testing-requirements)
10. [Code Style & Linting](#code-style--linting)
11. [Contributing ML Models or Scoring Changes](#contributing-ml-models-or-scoring-changes)
12. [Contributing a Plugin (Threat Detector)](#contributing-a-plugin)
13. [Documentation Contributions](#documentation-contributions)
14. [Issue Labels Reference](#issue-labels-reference)
15. [Review SLA & What to Expect](#review-sla)
16. [License & Developer Certificate of Origin](#license--dco)

---

## Code of Conduct

This project follows the [Contributor Covenant v2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/).

**TL;DR:** Be direct, be kind, disagree on ideas not on people. Maintainers may remove, edit, or reject contributions that violate this, no questions asked.

Report violations privately to: `conduct@vyse-security.dev` (or open a private GitHub Security Advisory if that address is unavailable).

---

## The Architecture

Before contributing, understand the five moving parts. A change to one can silently break another, especially across the gRPC boundary.

```
┌──────────────────────────────────────────────────────────────┐
│  gateway/              Go (Gin / Fiber)                      │
│  Request ingestion, JWT auth, rate limiting, session init    │
│  Talks to: engine via gRPC                                   │
├──────────────────────────────────────────────────────────────┤
│  engine/               Rust (Axum / Tokio)                   │
│  Behavioral scoring, ML inference, defence pipeline, Rekor   │
│  Talks to: Redis (session state), PostgreSQL (logs), Rekor   │
├──────────────────────────────────────────────────────────────┤
│  proto/                Protobuf definitions                  │
│  Service contracts between gateway and engine                │
│  Generated code must be committed — see §Working with gRPC   │
├──────────────────────────────────────────────────────────────┤
│  infra/                Redis + PostgreSQL                    │
│  Redis: hot-path session state, velocity counters            │
│  PostgreSQL: persistent attack logs, analytics               │
├──────────────────────────────────────────────────────────────┤
│  dashboard/            React SPA                             │
│  Real-time monitoring, audit verification                    │
│  Talks to: gateway admin REST endpoints + WebSocket          │
└──────────────────────────────────────────────────────────────┘
```

### Critical Paths

Changes to the following require extra scrutiny and dedicated tests. Expect a slower review and harder questions.

| Path | Language | Why It's Critical |
|---|---|---|
| `engine/src/scoring/` | Rust | Determines tier. Wrong logic = false negatives (attacks through) or false positives (legit users degraded). |
| `engine/src/defence/pipeline.rs` | Rust | Applies perturbation. A bug here serves clean responses to Tier 3 actors. |
| `engine/src/audit/rekor.rs` | Rust | Immutability. A silent bug loses the forensic audit trail permanently. |
| `gateway/internal/middleware/auth.go` | Go | Auth bypass = full admin exposure to the internet. |
| `proto/vyse.proto` | Protobuf | Breaks the gateway↔engine contract. Requires coordinated changes in both services. |
| `engine/src/store/session.rs` | Rust | Redis session state. Corruption here creates ghost tiers and incorrect scoring. |

### Data Flow at a Glance

```
Client → [Go Gateway] → gRPC → [Rust Engine]
                                     │
                         ┌───────────┼───────────┐
                         ↓           ↓           ↓
                       Redis      ML Models   PostgreSQL
                   (session state)(inference) (audit logs)
                                     │
                                     ↓
                                   Rekor
                              (Tier 3 events)
```

---

## Ways to Contribute

### Not a coder? Still useful.

- **Reproduce a bug** — add a comment to an existing issue with a fresh reproduction
- **Improve docs** — spotted an ambiguity, wrong command, or missing step? Fix it
- **Test an attack pattern** — run the attack simulator and report unexpected tier behaviour
- **Review open PRs** — community review is explicitly welcomed and appreciated
- **Translate** — Vyse should be accessible globally

### Coder? Pick your track.

| Track | Language | Good entry points |
|---|---|---|
| Bug fixes | Go or Rust | Issues labelled `bug` + `good first issue` |
| Gateway features | Go | Issues labelled `gateway` |
| Scoring improvements | Rust | Issues labelled `scoring` — requires ML understanding |
| Defence / perturbation | Rust | Issues labelled `defence` |
| ML model / inference | Rust | Issues labelled `ml-inference` — requires Rust + ONNX/candle familiarity |
| Redis / session state | Rust | Issues labelled `redis` |
| gRPC / proto definitions | Protobuf | Issues labelled `grpc` |
| Plugin development | Rust | See [Contributing a Plugin](#contributing-a-plugin) |
| Dashboard / frontend | React / JS | Issues labelled `dashboard` |
| Infrastructure / DB | SQL / Docker | Issues labelled `infra` |
| Tests | Go or Rust | Issues labelled `test-coverage` — always needed |

### Not sure where to start?

Look for [`good first issue`](../../labels/good%20first%20issue) — scoped to 1–2 hours, clear outcome, no critical paths involved.

If you know Go but not Rust (or vice versa), filter by the `gateway` or `engine` label. The two services are independently workable.

---

## Security Vulnerability Disclosure

> Vyse is a security tool. We take our own security seriously.

**Do NOT open a public GitHub issue for security vulnerabilities.** This exposes the flaw before a fix exists.

Instead:

1. Go to **Security → Report a Vulnerability** on the GitHub repo page
2. Or email `security@vyse-security.dev` with PGP encryption (key on our website)
3. Include: affected component, reproduction steps, and your severity assessment

We will acknowledge within **48 hours**, provide a fix timeline within **5 business days**, and credit you in the security advisory unless you prefer anonymity.

### Special Case: Bypass Techniques

If you discover a technique that reliably evades Vyse's tier classification — a "bypass" — this is a **security vulnerability**, not a feature request. Report it privately. We will treat it with the same urgency as a code flaw, fix it, and credit you.

---

## Getting Started

### Prerequisites

| Tool | Minimum Version | Purpose |
|---|---|---|
| Go | 1.22+ | Gateway |
| Rust | stable (1.77+) | Engine |
| `protoc` | 25.x+ | Protobuf code generation |
| `protoc-gen-go` + `protoc-gen-go-grpc` | latest | Go gRPC stubs |
| Docker + Docker Compose | v2+ | Full stack local dev |
| Redis | 7+ | Session state (via Docker is fine) |
| PostgreSQL | 15+ | Persistent storage (via Docker is fine) |
| Git | 2.35+ | For `--force-with-lease` support |

Install Rust via [rustup](https://rustup.rs/). Install Go from [go.dev/dl](https://go.dev/dl/). Install `protoc` via your system package manager or from [github.com/protocolbuffers/protobuf/releases](https://github.com/protocolbuffers/protobuf/releases).

```bash
# Verify everything before starting
go version           # go1.22.x
rustc --version      # rustc 1.77.x
protoc --version     # libprotoc 25.x
docker compose version
```

### Fork & Clone

```bash
# Fork on GitHub first, then:
git clone https://github.com/YOUR_USERNAME/vyse.git
cd vyse
git remote add upstream https://github.com/vyse-security/vyse.git
```

### Infrastructure (Start This First)

Redis and PostgreSQL are required by both the engine and all integration tests. The easiest path is Docker:

```bash
# Start Redis and PostgreSQL
docker compose up -d redis postgres

# Verify both are healthy
docker compose ps

# Run database migrations
cd infra && make migrate-up
```

### Engine (Rust)

```bash
cd engine

# Copy config
cp .env.example .env
# Minimum required: GROQ_API_KEY, REDIS_URL, DATABASE_URL

# Download ML models — first time only, ~125MB total
# Exports all-MiniLM-L6-v2 and nli-MiniLM2-L6-H768 to ONNX format
make download-models

# Build in debug mode (fast compile, slower runtime)
cargo build

# Run
cargo run -- --config config.toml

# Hot-reload during development
cargo install cargo-watch
cargo watch -x run
```

The engine binds to `localhost:50051` (gRPC) by default.

### Gateway (Go)

```bash
cd gateway

# Download dependencies
go mod download

# Copy config
cp .env.example .env
# Minimum required: ENGINE_GRPC_ADDR, JWT_SECRET, VYSE_API_KEY

# Run
go run ./cmd/vyse-gateway

# Hot-reload (install air first)
go install github.com/air-verse/air@latest
air
```

The gateway binds to:
- `localhost:8080` — public inference endpoint
- `localhost:8081` — admin REST + WebSocket (JWT-protected)

### Dashboard (React)

```bash
cd dashboard
npm install
cp .env.example .env
npm start   # http://localhost:3000
```

### Full Stack (Docker — Recommended for Integration Testing)

```bash
cp .env.example .env
# Fill in: GROQ_API_KEY, JWT_SECRET, ADMIN_PASSWORD, VYSE_API_KEY

docker compose up --build
# Dashboard:         http://localhost:3000
# Gateway public:    http://localhost:8080
# Gateway admin:     http://localhost:8081
# Engine (gRPC):     localhost:50051
# PostgreSQL:        localhost:5432
# Redis:             localhost:6379
```

### Run the Attack Simulator

```bash
# Requires the full stack to be running
cd tools
go run ./attack-simulator --user attack-test-001 --duration 15 --rps 2
# Watch the dashboard escalate through tiers in real time
```

---

## Working with gRPC & Protobuf

The gateway and engine communicate exclusively via gRPC. The service contract lives in `proto/vyse.proto`. This file is **the source of truth** for the gateway↔engine interface.

### If You Need to Change the Proto Definition

1. Edit `proto/vyse.proto`
2. Regenerate all stubs:
   ```bash
   make proto-gen
   # Runs protoc for both Go (gateway stubs) and Rust (engine stubs)
   # Generated files are committed to the repo — do not .gitignore them
   ```
3. Update the Go gateway stubs in `gateway/internal/grpc/`
4. Update the Rust engine stubs in `engine/src/proto/`
5. Run integration tests: `make test-integration`

### Rules for Proto Changes

- **Never remove or renumber existing fields.** Protobuf uses field numbers for wire encoding. Removing or reusing a field number silently corrupts data in mixed-version deployments.
- **Adding new optional fields is backwards-compatible.** Safe to do without coordination.
- **Changing a field type is a breaking change.** Treat it like `BREAKING CHANGE:` in your commit and document the migration path.
- Every `.proto` change must include updated generated stubs in the same commit. PRs with stale generated code are rejected.

### Generated Code Policy

Generated gRPC stubs (`*.pb.go`, `*.pb.rs`) are **committed to the repository**. Do not add them to `.gitignore`. This ensures contributors can build without running `protoc` locally, and CI can detect drift between proto definitions and generated stubs.

---

## Branching & Commit Conventions

### Branch Naming

Branches **must** follow this pattern: `<type>/<short-description>`

```
feat/isolation-forest-a-score
fix/tier3-rekor-submission-timeout
fix/redis-session-ttl-race-condition
docs/plugin-contribution-guide
test/scoring-e-score-edge-cases
chore/upgrade-ort-2.x
refactor/defence-pipeline-seed-locking
proto/add-session-metadata-field
```

Never push directly to `main`. All changes enter through pull requests.

### Commit Message Format

Vyse follows [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

```
<type>(<scope>): <short summary in imperative mood>

[optional body — wrap at 72 chars]

[optional footer: BREAKING CHANGE / Closes #issue]
```

**Allowed types:**

| Type | When to Use |
|---|---|
| `feat` | A new user-visible or API-visible feature |
| `fix` | A bug fix |
| `docs` | Documentation only |
| `test` | Adding or fixing tests only — no production code change |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | A measurable performance improvement |
| `chore` | Build process, dependency updates, CI changes |
| `security` | Security hardening not classified as a bug fix |
| `proto` | Changes to `.proto` definitions and generated stubs |

**Allowed scopes:** `gateway`, `engine`, `dashboard`, `scoring`, `defence`, `rekor`, `auth`, `redis`, `postgres`, `grpc`, `proto`, `plugin`, `docker`, `docs`, `ci`, `infra`

**Examples:**

```
feat(scoring): add E-Score bigram entropy signal

Adds a third scoring signal measuring session query entropy.
Low entropy across the last 20 queries indicates repetitive
extraction patterns that V-Score and D-Score can miss.

Closes #42
```

```
fix(redis): prevent race condition on concurrent session writes

Replace GET-then-SET with atomic WATCH/MULTI/EXEC transaction
for session embedding updates. Prevents score corruption when
two requests from the same session arrive within the same
millisecond under high load.

Closes #77
```

```
proto(grpc): add session_metadata field to InferenceRequest

BREAKING CHANGE: gateway must populate session_metadata or
engine returns INVALID_ARGUMENT. Deploy engine update before
gateway update. See migration guide in docs/migration/v0.4.md.
```

**Rules:**
- **Imperative mood** in summary: "add", "fix", "remove" — not "added", "fixes"
- Summary under **72 characters**
- Reference issues in the footer: `Closes #N` or `Ref #N`
- `proto` commits must include both the `.proto` change and the regenerated stubs

### Keeping Your Branch Updated

```bash
git fetch upstream
git rebase upstream/main

# Force-push after rebase if needed — always use --force-with-lease
git push origin your-branch --force-with-lease
```

Never `git push --force`. Only ever `--force-with-lease`.

---

## Pull Request Process

### Before Opening a PR

- [ ] `make test` passes — full test suite in both Rust and Go
- [ ] `make lint` passes — no errors in `cargo clippy` or `golangci-lint`
- [ ] If you changed `proto/`, regenerate stubs with `make proto-gen` and commit them
- [ ] If you changed `engine/`, verify release build: `cargo build --release`
- [ ] If you changed `gateway/`, verify: `go build ./...`
- [ ] If your change affects tier logic, run the attack simulator and verify escalation behaviour
- [ ] `CHANGELOG.md` updated under `[Unreleased]`
- [ ] Relevant documentation updated if user-facing behaviour changed

### PR Description Template

When you open a PR, a template loads automatically. Fill all sections. It asks for:

- **What** — description of the change
- **Why** — motivation; link the issue
- **How** — non-obvious implementation decisions
- **Test plan** — what you tested and how to reproduce
- **Security considerations** — required for changes to `scoring/`, `defence/`, `auth/`, `rekor/`, `proto/`
- **Breaking changes** — if any, include migration notes
- **Proto changes** — confirm generated stubs are included if `.proto` was modified

### PR Scope

One logical change per PR. A PR touching the proto definition, scoring logic, Redis session handling, and the dashboard simultaneously is too broad. Split it unless the changes are genuinely inseparable.

### Review Process

- Minimum **1 maintainer approval** to merge
- PRs touching critical paths require **2 approvals**
- CI must pass: Rust tests, Go tests, lint, proto drift check, Docker build, integration tests
- All reviewer comments must be resolved or explicitly acknowledged

A maintainer merges once approved. Contributors do not self-merge.

---

## Testing Requirements

Three test layers. All three must pass before any merge.

### Unit Tests

**Rust (Engine):**
```bash
cd engine
cargo test                          # all unit tests
cargo test scoring::                # scoring module only
cargo test defence::                # defence module only
cargo test -- --nocapture           # with stdout (useful for debugging)
```

**Go (Gateway):**
```bash
cd gateway
go test ./...                       # all packages
go test ./internal/middleware/...   # auth middleware only
go test -race ./...                 # with race detector — run this before every PR
```

**Coverage floors (enforced by CI):**

| Component | Minimum Line Coverage |
|---|---|
| `engine/src/scoring/` | 85% |
| `engine/src/defence/` | 80% |
| `engine/src/audit/` | 80% |
| `gateway/internal/middleware/` | 85% |
| `gateway/internal/grpc/` | 75% |

```bash
# Rust coverage
cargo install cargo-llvm-cov
cargo llvm-cov --html

# Go coverage
go test -coverprofile=coverage.out ./...
go tool cover -html=coverage.out
```

### Integration Tests

Exercise the full gateway → gRPC → engine → Redis → PostgreSQL path. Requires both Redis and PostgreSQL running.

```bash
# Start infra first
docker compose up -d redis postgres

make test-integration
```

The Rekor client is mocked by default. To test against the live public log:
```bash
REKOR_URL=https://rekor.sigstore.dev make test-integration
```

### End-to-End Tests

Spins up the full Docker Compose stack, runs the attack simulator, and asserts on tier progression and database state.

```bash
make test-e2e   # ~3 minutes
```

Run this before any PR touching Docker configuration, gRPC contracts, or tier logic.

### What to Test — By Change Type

**New scoring signal:**
- First request (no prior session context)
- Score exactly at the tier transition boundary
- Empty / malformed / extremely long inputs
- A normal-looking benign session must not cross Tier 2

**New Redis operation:**
- Concurrent writes from the same session — run with `cargo test -- --test-threads=8`
- TTL expiry mid-request — what happens when the session key disappears?
- Redis unavailable — the engine must return a proper error, not panic

**Proto change:**
- Old gateway against new engine (backwards compat)
- New gateway against old engine (forwards compat if required)

**Defence pipeline change:**
- Seed-locked: identical input + identical seed must produce identical output across 100 runs
- Tier 1 sessions must receive completely unperturbed clean responses
- Very short responses (1 word, 1 token) must not panic or produce empty output

---

## Code Style & Linting

### Rust (Engine)

```bash
cd engine

# Format — must pass in CI
cargo fmt --check
cargo fmt              # auto-fix

# Lint — all warnings are errors in CI
cargo clippy -- -D warnings
```

**Rust-specific rules:**

- Use `thiserror` for error types — no `Box<dyn Error>` in library code
- All `async` functions in the hot path must carry `#[instrument]` (tracing crate)
- No `unwrap()` or `expect()` in production code — use `?` with typed errors. Exceptions: test code, and genuinely unreachable branches with a `// SAFETY:` comment
- `unsafe` blocks require a `// SAFETY:` comment explaining the upheld invariants
- Prefer `Arc<T>` for state shared across async tasks
- All Redis operations must handle connection failures by returning `Err(...)`, never by panicking

### Go (Gateway)

```bash
cd gateway

gofmt -l .             # list files needing formatting
gofmt -w .             # auto-format
golangci-lint run ./...
```

Install: `go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest`

**Go-specific rules:**

- All exported functions, types, and methods must have godoc comments
- Error strings lowercase, no trailing punctuation — Go convention
- No `log.Fatal` or `os.Exit` outside of `main()` — propagate errors upward
- `context.Context` is always the first parameter when accepted
- gRPC handlers must propagate context cancellation — check `ctx.Done()` in loops
- Use `errors.Is` and `errors.As` — never compare error strings directly

### Protobuf

- Field names: `snake_case`
- Message and service names: `PascalCase`
- Every field must have a comment
- Removed fields must use `reserved` with a comment explaining why

### React (Dashboard)

```bash
cd dashboard
npm run lint
npm run format
```

### Universal Rules

- No hardcoded credentials, tokens, or secrets anywhere — ever
- No commented-out code in merged PRs
- All configuration via environment variables or `config.toml` — never hardcoded
- Structured logging only: Go uses `slog`, Rust uses `tracing` — no `fmt.Println` or `println!` in production code

---

## Contributing ML Models or Scoring Changes

### How ML Inference Works in Vyse

The Rust engine runs all ML inference natively using one of two backends:

- **ONNX Runtime (`ort` crate)** — default. Cross-platform, well-supported, fastest for inference.
- **Candle (HuggingFace)** — alternative. Pure Rust, no C++ runtime dependency, slightly slower.

Models are exported to ONNX format during `make download-models` and loaded at engine startup. If you are proposing a new model, you must provide an ONNX export and a verified parity check — see requirements below.

### Why Scoring Changes Are Gated

A change to the embedding model, anomaly detector, or NLI classifier can:

- Increase **false positives** — legitimate users receive degraded, perturbed responses
- Increase **false negatives** — attackers evade detection entirely
- Introduce **adversarial exploitability** — a new model may be more sensitive to adversarial inputs

### Required Before Proposing a Model Change

1. **Open an issue first.** Label it `scoring` + `rfc`. Describe the proposed change, motivation, and preliminary evidence. Wait for maintainer sign-off before writing code.

2. **Benchmark against the current model.** Run `tools/scoring_benchmark.py` against both models. Include the output in your PR. The benchmark reports precision/recall on tier assignment against the standard attack corpus.

3. **License check.** Vyse is MIT licensed. The model must be Apache-2, MIT, or BSD.

4. **CPU performance.** The ONNX export must run inference within **50ms per request** on a 2-core machine. Rust inference via `ort` is significantly faster than Python — if your model needs 150ms, that is a regression.

5. **Size limit.** ONNX export under 500MB.

6. **ONNX parity check.** Run `tools/models/verify_onnx.py` and include the output. Cosine similarity between original Python outputs and ONNX outputs on 100 test inputs must be ≥ 0.999.

### Acceptable Proposals

- Replacing `all-MiniLM-L6-v2` for D-Score: only if the benchmark shows improved attack/benign separation
- New scoring signal: requires issue, RFC discussion, and benchmark before implementation
- Updating a model version: verify ONNX parity and that all tests pass

---

## Contributing a Plugin

Vyse's plugin system allows external threat detector modules. A plugin adds a new score signal, input filter, or post-processing step without touching core engine code.

### Plugin Interface (Rust Trait)

```rust
// All plugins must implement this trait.
// Full type definitions: engine/src/plugin/mod.rs

use async_trait::async_trait;
use vyse_engine::plugin::{PluginContext, PluginError, PluginResult, VyseThreatPlugin};

pub struct MyDetector;

#[async_trait]
impl VyseThreatPlugin for MyDetector {
    fn name(&self) -> &'static str {
        "my-detector"   // unique slug: lowercase, hyphens only
    }

    fn version(&self) -> &'static str { "1.0.0" }

    fn license(&self) -> &'static str {
        "MIT"           // must be MIT, Apache-2, or BSD
    }

    async fn evaluate(&self, ctx: &PluginContext) -> Result<PluginResult, PluginError> {
        // ctx: prompt, user_id_hash, session_history, base_scores
        // Return: score in [0.0, 1.0] + optional metadata HashMap
        todo!()
    }
}
```

### Plugin PR Requirements

- Plugin lives in `plugins/<your-plugin-name>/` as its own Cargo crate
- Include `README.md` explaining what it detects and why
- Include unit tests — at minimum, 3 benign inputs and 3 malicious inputs
- No blocking calls in `evaluate()` — async IO only
- All dependencies declared in the plugin's own `Cargo.toml`
- Plugins using ML models: follow the [ML model requirements](#contributing-ml-models-or-scoring-changes) above

### Good Plugin Ideas

- IP reputation lookup (offline, MaxMind GeoLite2 FOSS DB)
- Prompt injection pattern detection (regex + semantic hybrid)
- Time-of-day anomaly detection
- Cross-session correlation (same attacker across different session IDs)
- Language consistency check (mid-session language shifts typical of automated probing)

---

## Documentation Contributions

Documentation lives in `docs/` (MkDocs) and inline in source.

### What Counts as Documentation

- `docs/` — user guides, architecture deep dives, deployment recipes
- `CONTRIBUTING.md` — this file
- Rust doc comments (`///`) on all public items in `engine/src/`
- Go doc comments on all exported symbols in `gateway/`
- Protobuf field comments in `proto/vyse.proto`
- Inline comments explaining *why*, not *what*
- `CHANGELOG.md` — updated with every PR

### Documentation PRs

Pure documentation PRs are fast-tracked. Only the docs build check is required:

```bash
cd docs && mkdocs build --strict
```

Spell-checking is enforced by CI via `cspell`. Domain-specific false positives can be added to `docs/.cspell.json`.

### Changelog Format

Vyse follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

```markdown
## [Unreleased]

### Added
- A-Score anomaly detection via IsolationForest over session feature vectors (#42)

### Changed
- Engine migrated from Python/FastAPI to Rust/Axum — p99 latency reduced to 10–25ms (#55)
- Internal gateway↔engine communication now uses gRPC instead of REST (#55)
- Session state moved from SQLite to Redis for sub-millisecond hot-path lookups (#60)

### Fixed
- Redis WATCH/MULTI/EXEC prevents race condition on concurrent session writes (#77)

### Security
- Gateway requires X-Vyse-Key on all inference endpoints (#95)

### Breaking Changes
- Session header renamed from X-User-ID to X-Vyse-Session (#95)
- Engine configuration moved from .env to config.toml (#102)
```

---

## Issue Labels Reference

| Label | Meaning |
|---|---|
| `bug` | Something is broken |
| `good first issue` | Self-contained, no critical paths, 1–2 hour scope |
| `help wanted` | Maintainers want community input |
| `scoring` | V/D/E/A score or tier logic |
| `defence` | Perturbation pipeline |
| `rekor` | Transparency log / audit trail |
| `gateway` | Go gateway — auth, rate-limiting, routing |
| `engine` | Rust engine internals |
| `redis` | Session state, velocity counters, hot-path ops |
| `postgres` | Persistent storage, migrations, analytics queries |
| `grpc` | gRPC service definitions or generated stubs |
| `proto` | Protobuf definition changes |
| `ml-inference` | ONNX/candle model loading, inference latency |
| `dashboard` | React dashboard |
| `plugin` | Plugin system |
| `rfc` | Request for comments — design discussion before code |
| `security` | Security issue — maintainer-only until patched |
| `test-coverage` | Missing or insufficient tests |
| `docs` | Documentation improvement |
| `breaking` | Introduces a breaking API, config, or proto change |
| `needs-repro` | Bug report needs a reproduction case |
| `performance` | Latency, throughput, or memory regression |
| `wontfix` | Out of scope or intentionally unchanged |

---

## Review SLA

Maintainers aim for:

| PR Type | First Response | Merge Target |
|---|---|---|
| Documentation only | 2 business days | 3 business days |
| Bug fix (non-critical path) | 3 business days | 1 week |
| Bug fix (critical path) | 1 business day | 3 business days |
| Proto / gRPC change | 2 business days | 1 week (requires coordinated engine + gateway review) |
| New feature | 5 business days | 2 weeks |
| Scoring / model change | 1 week | After RFC discussion closes |
| Security fix | 24 hours | ASAP |

These are targets, not guarantees. If your PR has not received a response past the SLA, comment on it and tag `@vyse-security/maintainers`.

---

## License & DCO

### License

Vyse is licensed under the **MIT License**. By contributing, you agree that your contributions will be licensed under the same terms. All plugin contributions must also be MIT, Apache-2, or BSD licensed.

### Developer Certificate of Origin (DCO)

All commits must be signed off, certifying you wrote the code and have the right to contribute it under the MIT license.

```bash
git commit --signoff -m "feat(scoring): add E-Score signal"
# shorthand:
git commit -s -m "feat(scoring): add E-Score signal"
```

This appends `Signed-off-by: Your Name <your@email.com>`. The DCO check is enforced in CI — unsigned commits block the PR.

Full DCO text: [developercertificate.org](https://developercertificate.org/)

### No CLA Required

Vyse does not require a Contributor License Agreement. The DCO sign-off is sufficient. CLAs create unnecessary friction for open-source contributors.

---

*Thank you for reading this far. That already puts you ahead of most contributors. Now go build something.*
