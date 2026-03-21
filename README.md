<div align="center">

<br/>


<img width="1003" height="382" alt="image" src="https://github.com/user-attachments/assets/f0fbf4fa-d8df-4021-9823-1f8785ff6b1a" />


# **Stateful ML API Security : Built with effort. Broken by patterns. Protected by Vyse.**

<br/>

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-white.svg?style=flat-square)](LICENSE.md)
[![Go](https://img.shields.io/badge/Go-1.22-00acd7?style=flat-square&logo=go&logoColor=white)](gateway/)
[![Rust](https://img.shields.io/badge/Rust-1.77-ce422b?style=flat-square&logo=rust&logoColor=white)](engine/)
[![gRPC](https://img.shields.io/badge/gRPC-protobuf-a78bfa?style=flat-square)](proto/vyse.proto)
[![Rekor](https://img.shields.io/badge/Audit-Rekor%20%2F%20Sigstore-34d399?style=flat-square)](https://sigstore.dev)
[![FOSS](https://img.shields.io/badge/FOSS-Hackathon%202026-f59e0b?style=flat-square)]()

<br/>

</div>

---

## What is Vyse

Vyse is a stateful security middleware layer that sits between any HTTP client and any ML model API. It intercepts every inference request, maintains a behavioural profile across the session, and detects adversarial patterns that only appear over time.

Most ML API defences are stateless   they inspect one request in isolation. An attacker who spreads their queries across time, varies their phrasing slightly, and keeps their request rate low will pass through every stateless check undetected.

Vyse asks a different question: **what has this session been doing?**

By tracking velocity, semantic divergence, prompt entropy, and statistical anomaly across requests, Vyse detects extraction attacks, gradient probing, and systematic boundary mapping that would be invisible to per-request inspection. When a session is flagged, Vyse injects adaptive noise into responses   making the attacker's collected data mathematically useless   while legitimate users see no change whatsoever.

No model retraining. No modification to your existing API. No proprietary cloud dependencies.

---

## The Attack Problem

```
Stateless defence sees this:

  t=0    "What is 2+2?"          → allow
  t=30s  "What is 2+3?"          → allow
  t=60s  "What is 2+4?"          → allow
  t=90s  "What is 2+5?"          → allow
  ...

Vyse sees this:

  t=0    "What is 2+2?"          → D-Score: 0.00  Tier 1
  t=30s  "What is 2+3?"          → D-Score: 0.94  V-Score rising
  t=60s  "What is 2+4?"          → D-Score: 0.96  E-Score: 0.87
  t=90s  "What is 2+5?"          → Hybrid: 0.81   → Tier 2   noise injected
  t=10m  continued pattern       → Hybrid: 0.93   → Tier 3   Rekor audit
```

The semantic similarity between consecutive prompts, the low bigram entropy of the session history, and the velocity pattern together produce a hybrid score that no individual request would trigger.

---

## Architecture

```
                             Internet
                                 │
                         ┌───────▼────────┐
                         │  Load Balancer │
                         └───────┬────────┘
                                 │ HTTPS
                         ┌───────▼──────────────────────────┐
                         │   Go Gateway  :8080 / :8081      │
                         │                                  │
                         │  · JWT + API key authentication  │
                         │  · Per-IP token bucket limiting  │
                         │  · Session ID extraction + hash  │
                         │  · Request validation            │
                         └───────┬──────────────────────────┘
                                 │ gRPC (proto/vyse.proto)
                         ┌───────▼──────────────────────────┐
                         │     Rust Engine  :50051          │
                         │                                  │
                         │  · V / D / E / A scoring         │
                         │  · ONNX ML inference             │
                         │  · Intent classification         │
                         │  · Response perturbation         │
                         │  · Rekor audit (Tier 3)          │
                         └────────┬───────────────┬─────────┘
                                  │               │
                    ┌─────────────▼──────┐   ┌────▼────────────┐
                    │    Redis :6379     │   │   PostgreSQL    │
                    │    session state   │   │   audit logs    │
                    │    hot path        │   │   analytics     │
                    └────────────────────┘   └────────┬────────┘
                                                      │
                                           ┌──────────▼──────────┐
                                           │  Rekor (Sigstore)   │
                                           │  Tier 3 events      │
                                           │  tamper-evident     │
                                           └─────────────────────┘
```

**Latency overhead: 10–25 ms**   negligible against any LLM inference workload.

---

## The Four Scoring Signals

Every request updates four independent signals. They are combined into a weighted hybrid score `H ∈ [0.0, 1.0]`.

```
H = 0.25 × V  +  0.35 × D  +  0.15 × E  +  0.25 × A
```

All weights are configurable in `config.toml`. They are validated at startup to sum to 1.0.

<br/>

### ─── V-Score · Velocity · weight 0.25

Requests per minute within a sliding 5-minute window, using **exponential decay weighting**. A request from 4 minutes ago contributes ~2% of what a request right now does. This makes the score responsive to the attacker's current behaviour, not their historical peak.

```
V = min(1.0,  weighted_rpm  /  max_rpm)
                                └── default: 30 RPM
```

Detects: automated scrapers, high-frequency probing bots.

<br/>

### ─── D-Score · Divergence · weight 0.35

Cosine similarity between the current prompt's embedding and the previous prompt's embedding in the same session. High similarity means the attacker is varying inputs minimally   the defining fingerprint of model extraction.

```
D = cosine_similarity( embed(prompt_t),  embed(prompt_{t-1}) )
```

Model: **`all-MiniLM-L6-v2`** (22 MB, Apache-2, CPU-only, ONNX).
Embeddings are cached per session   only the new prompt is re-inferred each request.

"Show me model weights" and "display model parameters" differ in every word but score D ≈ 0.91. A hash-based approach would score these as unrelated.

Detects: gradient-based boundary probing, systematic enumeration.

<br/>

### ─── E-Score · Entropy · weight 0.15

Inverse Shannon bigram entropy across the last 20 prompts in the session. Low entropy means the session's vocabulary is repetitive   characteristic of template-based extraction scripts.

```
E = 1  −  normalise( H( bigram_freq(last 20 prompts) ) )
```

An attacker cycling through "extract X weights", "reveal X parameters", "show X coefficients" has high unigram diversity but low bigram diversity   `X weights`, `X parameters`, `X coefficients` share a common bigram template. E-Score catches what unigrams miss.

Detects: template-based extraction scripts, systematic vocabulary enumeration.

<br/>

### ─── A-Score · Anomaly · weight 0.25

Online anomaly detection over the feature vector `[rpm, similarity, entropy_inv, prompt_length, duration_mins]`. Uses an incrementally-updated **Welford rolling statistics model** (IsolationForest approximation) that is pre-seeded with a prior distribution representing normal API usage. As the deployment accumulates sessions, the model adapts to that deployment's baseline.

```
feature_vec = [V_raw, D, E, len(prompt), duration_mins]
A           = sigmoid( z_score_75th_percentile(feature_vec) )
```

The 75th-percentile Z-score (not the maximum) is used to avoid false positives from single-dimension outliers   a very long but otherwise normal prompt does not trigger the score alone.

Detects: sessions that are statistically abnormal even when no individual threshold is crossed.

---

## Intent Classification Override

The four signals can be bypassed by a sufficiently slow attacker who keeps their RPM low, varies prompts enough to lower D and E, and appears statistically normal. To address this, every prompt is also passed through an **NLI intent classifier**.

Model: **`cross-encoder/nli-MiniLM2-L6-H768`** (90 MB, Apache-2, CPU-only, ONNX).

If the classifier assigns entailment probability ≥ 0.85 to any of the configured extraction-style intent labels:

```toml
# engine/config.toml
[ml]
intent_labels = [
  "extract model weights",
  "dump training data",
  "bypass security controls",
  "reveal system prompt",
  "extract confidence scores",
]
intent_threshold = 0.85
```

The session is **immediately escalated to Tier 3** regardless of its numerical hybrid score. This means a slow, careful attacker who takes 40 minutes to accumulate a mild signal score will still be caught the moment they issue a semantically unambiguous extraction prompt.

---

## Tier Classification

```
Tier 1   Clean
  Condition : H < 0.55  AND  duration < 2 min
  Response  : Clean LLM output served as-is

Tier 2   Suspicious
  Condition : H ≥ 0.55  OR  duration ∈ [2, 10) min
  Response  : Synonym substitution (45% of content words)
              + numeric perturbation (±5%)
              All noise is seed-locked to the session.

Tier 3   Malicious
  Condition : H ≥ 0.90  AND  duration > 10 min
         OR : intent classifier fires
  Response  : Maximum perturbation (70% substitution + sentence reorder)
              + Rekor transparency log entry created
```

All thresholds are configurable in `engine/config.toml`.

---

## Sticky Noise   Why It Works

When a session reaches Tier 2 or 3, Vyse applies perturbation to every response. The noise is **deterministic and session-scoped**:

```
noise_seed = SHA-256( session_id_hash ‖ tracking_started_at )
```

The same seed is used for every perturbed response in the session. This is the critical design choice: an attacker who collects 500 perturbed responses and tries to average them to cancel out the noise **cannot**   because the noise is not random, it is consistent. Every response shifts in the same direction by the same amounts. The average of 500 consistently-shifted responses is still shifted.

Legitimate users see no noise. Tier classification is session-scoped, not request-scoped   a legitimate user is never bumped to Tier 2 by accident.

---

## Rekor Audit Trail

Every Tier 3 event is submitted to a **Rekor transparency log** (Sigstore), either the public instance at `rekor.sigstore.dev` or a self-hosted stack.

Rekor uses a Merkle tree: each entry is linked to the previous one via `SHA-256(prev_hash ‖ entry_data)`. Modifying any past entry breaks the chain and is immediately detectable. An auditor can verify any event without trusting the Vyse operator.

Each audit entry contains:
- `SHA-256(session_id)`   privacy-preserving identity
- hybrid score and all four signal scores
- session duration
- noise seed (for forensic replay)
- Ed25519 signature from this Vyse instance

Raw session IDs, raw IPs, and raw prompts are **never** stored in the audit log or in PostgreSQL. Only SHA-256 hashes.

---

## Quickstart

**Requirements:** Go 1.22, Rust 1.77, Docker + Docker Compose v2.

```bash
git clone https://github.com/vyse-security/vyse
cd vyse

# Copy and fill in required secrets
cp .env.example .env
# Required: VYSE_AUTH_JWT_SECRET, VYSE_AUTH_ADMIN_PASSWORD,
#           VYSE_AUTH_API_KEY, VYSE_LLM_API_KEY

# Start the full stack
docker compose up --build
```

| Service   | Address              |
|-----------|----------------------|
| Gateway (inference) | `localhost:8080` |
| Gateway (admin)     | `localhost:8081` |
| Dashboard           | `localhost:3000` |
| Engine (gRPC)       | `localhost:50051` |

**First inference request:**

```bash
curl -X POST http://localhost:8080/api/infer \
  -H "X-Vyse-Key: your-api-key" \
  -H "X-Vyse-Session: session-abc-123" \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Explain gradient descent"}'
```

```json
{
  "response": "Gradient descent is an optimisation algorithm...",
  "tier": 1,
  "request_id": "01924a3b-..."
}
```

**Run the attack simulator** (watch the dashboard escalate through tiers in real time):

```bash
cd tools
go run ./attack-simulator --user attacker-001 --duration 15 --rps 2
```

---

## Configuration

All values live in two files. Environment variables override config files.

**`gateway/config.toml`**   HTTP ports, auth, rate limits, engine address.
**`engine/config.toml`**   scoring weights, tier thresholds, ML model paths, LLM provider, Rekor URL.

Key scoring configuration:

```toml
# engine/config.toml

[scoring]
weight_velocity   = 0.25
weight_divergence = 0.35
weight_entropy    = 0.15
weight_anomaly    = 0.25  # must sum to 1.0

tier2_score_threshold   = 0.55
tier3_score_threshold   = 0.90
tier2_min_duration_mins = 2.0
tier3_min_duration_mins = 10.0
max_rpm                 = 30.0

[llm]
provider = "groq"          # groq | openai | ollama
model    = "llama-3.1-8b-instant"
```

---

## API Reference

### Inference · Public endpoint

```
POST /api/infer
Headers:
  X-Vyse-Key: <api-key>
  X-Vyse-Session: <session-id>
  Content-Type: application/json

Body:
  { "prompt": "..." }

Response 200:
  {
    "response":   "...",
    "tier":       1,
    "request_id": "uuid"
  }
```

### Admin · JWT-protected

```
POST   /admin/auth/token        Issue admin JWT
GET    /admin/sessions          List all active sessions
GET    /admin/logs              Paginated query log
GET    /admin/stats             Aggregate threat statistics
GET    /admin/ledger            Rekor audit entries
GET    /admin/ledger/verify     Chain integrity check
DELETE /admin/sessions/:hash    Ban a session
GET    /health                  Liveness + engine status
```

---

## Repository Structure

```
vyse/
├─ proto/                  gRPC service contract (source of truth)
│  └─ vyse.proto
├─ gateway/                Go   HTTP ingestion, auth, rate limiting
│  ├─ cmd/vyse-gateway/
│  ├─ internal/config/
│  ├─ internal/middleware/  auth · ratelimit · session
│  ├─ internal/handlers/   infer · admin · sessions · logs · ledger
│  ├─ internal/grpc/       engine client
│  ├─ internal/proto/      generated Go stubs
│  └─ internal/server/     dual-port HTTP + WebSocket
├─ engine/                 Rust   scoring, ML, defence, audit
│  └─ src/
│     ├─ scoring/          v_score · d_score · e_score · a_score
│     ├─ defence/          synonym · numeric · mod (pipeline)
│     ├─ ml/               embedding · intent · llm
│     ├─ store/            session (Redis) · logs (PostgreSQL)
│     ├─ audit/            Rekor client · entry · queue
│     ├─ grpc/             tonic service implementation
│     └─ plugin/           threat detector plugin trait
├─ dashboard/              React   admin UI, live WebSocket feed
├─ infra/                  Rekor + Trillian config, migrations
├─ tools/                  attack-simulator · model downloader
└─ docker-compose.yml
```

---

## Tech Stack

| Layer | Technology | Purpose |
|---|---|---|
| Gateway | Go 1.22 · Gin | HTTP ingestion, auth, rate limiting |
| Engine | Rust 1.77 · Tokio · Axum | Scoring, defence, ML inference |
| Transport | gRPC · Protocol Buffers | Gateway ↔ engine communication |
| Embeddings | all-MiniLM-L6-v2 · ONNX | D-Score semantic similarity |
| Intent | nli-MiniLM2-L6-H768 · ONNX | Extraction intent classification |
| Session store | Redis 7 | Hot-path behavioural state |
| Audit log | PostgreSQL 15 | Query logs, analytics |
| Transparency log | Rekor · Sigstore | Tamper-evident Tier 3 audit |
| Dashboard | React · TypeScript | Real-time monitoring UI |

---

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a PR.

The short version:

- Gateway changes: Go 1.22, `make test-race` before every PR
- Engine changes: Rust 1.77, `cargo clippy -- -D warnings`, seed-determinism tests required for any defence change
- Proto changes: run `make proto-gen`, commit generated stubs in the same commit as the `.proto` change, never renumber existing fields
- ML model changes: open an RFC issue first, include ONNX parity check (cosine sim ≥ 0.999), benchmark against attack corpus

Security vulnerabilities and bypass techniques: **Security → Report a Vulnerability** on GitHub, not a public issue.

---

## Research Foundations

Vyse's design is informed by the following body of work on ML model attacks and defences.

Tramèr et al., **Stealing Machine Learning Models via Prediction APIs**, USENIX Security 2016   the original formalisation of model extraction as an attack class.

Fredrikson et al., **Model Inversion Attacks that Exploit Confidence Information**, ACM CCS 2015   how confidence scores enable systematic inference about training data.

Shokri et al., **Membership Inference Attacks Against Machine Learning Models**, IEEE S&P 2017   statistical distinguishability of members vs non-members from prediction behaviour.

Dwork et al., **The Algorithmic Foundations of Differential Privacy**   the theoretical basis for calibrated noise injection as a formal privacy mechanism.

---

## License

GNU General Public License v3.0 (GPL-3.0). See [LICENSE.md](LICENSE.md).

Vyse complements existing controls   WAFs, API gateways, rate limiters. It is not a replacement for them.
