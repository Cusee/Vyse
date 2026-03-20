# VYSE

**Stateful, open-source security middleware for Machine Learning APIs.**

VYSE sits between any HTTP client and any ML model, intercepting every request and analyzing it for adversarial intent before it reaches your model. It requires no model retraining, no modification to your existing API, and no proprietary cloud dependencies.

---

# The Problem

Most ML API defenses are stateless. They evaluate each request in isolation and ask:

*Is this single request suspicious?*

That question is easy to defeat. An attacker who spreads their probing queries across time, keeps their request rate low, and slightly varies their inputs will bypass stateless firewalls undetected.

VYSE asks a different question:

**What has this user been doing across their entire session?**

By maintaining behavioral state across requests, VYSE detects patterns that only appear over time. This shift from per-request inspection to time-stateful analysis makes VYSE significantly harder to evade than traditional API security tools.

---

# How It Works

VYSE maintains a behavioral profile for every active session. Each request updates that profile using four signals which are combined into a hybrid threat score.

The threat score determines the session’s **security tier**, and the tier determines which countermeasures are applied.

---

# The Four Behavioral Signals

### V-Score : Velocity (weight: 0.25)

Requests per minute within a sliding five-minute window using exponential decay weighting.

Purpose:

Detects high-frequency automated querying and scraping.

---

### D-Score : Divergence (weight: 0.35)

Cosine similarity between the current prompt embedding and the previous one.

High similarity indicates systematic probing using minimal input variation — a common model extraction technique.

Embedding model:


all-MiniLM-L6-v2


Embeddings are cached per session to minimize recomputation.

---

### E-Score — Entropy (weight: 0.15)

Bigram entropy across the last 20 prompts within a session.

Low entropy suggests template-based automated query generation typical of extraction scripts.

---

### A-Score — Anomaly (weight: 0.25)

IsolationForest anomaly detection over the feature vector:


[rpm, similarity, entropy, prompt_length]


Catches sessions that appear statistically abnormal even if they do not exceed individual thresholds.

---

# Intent Classification Override

Behavioral signals alone can be bypassed by slow attackers. To address this, VYSE applies an intent classifier to every prompt.

Model:


cross-encoder/nli-MiniLM2-L6-H768


If the classifier detects strong semantic alignment with extraction-style intents such as:

- extract model weights
- dump training data
- bypass security controls

the session is immediately escalated to **Tier 3**, regardless of its numerical threat score.

This layer prevents low-velocity adversaries from bypassing statistical defenses.

---

# Countermeasures

When a session enters a high-risk tier, VYSE applies **adaptive sticky noise** to model outputs.

Sticky noise means:


noise_seed = hash(session_id)


The perturbation remains consistent across the entire session.

This design prevents attackers from averaging responses across multiple queries to reconstruct the clean output.

Legitimate users receive clean responses with no degradation.

---

# System Architecture

VYSE is implemented as a layered security gateway placed in front of ML inference APIs.

The architecture is divided into two logical domains:


Defense Layer
↓
Model Layer


The defense layer intercepts and analyzes all traffic before forwarding requests to the model.

---

## High-Level Architecture

            Internet
                │
                ▼
       ┌─────────────────────┐
       │ Load Balancer       │
       └──────────┬──────────┘
                  │
                  ▼
       ┌─────────────────────┐
       │ Go API Gateway      │
       │---------------------│
       │ request ingestion   │
       │ JWT authentication  │
       │ rate limiting       │
       │ session creation    │
       └──────────┬──────────┘
                  │ gRPC
                  ▼
       ┌─────────────────────┐
       │ Rust Defense Engine │
       │---------------------│
       │ behavioral scoring  │
       │ embedding analysis  │
       │ anomaly detection   │
       │ intent classifier   │
       │ noise injection     │
       └──────────┬──────────┘
                  │
                  ▼
       ┌─────────────────────┐
       │ ML Model Layer      │
       │---------------------│
       │ LLM APIs            │
       │ local inference     │
       │ GPU clusters        │
       └─────────────────────┘

---

# Supporting Infrastructure

### Redis

Hot-path session store.

Stores:

- session embeddings
- velocity counters
- entropy windows
- threat scores

Redis ensures sub-millisecond session lookups.

---

### PostgreSQL

Persistent storage for:

- attack logs
- historical analytics
- dashboard queries

---

### Rekor (Sigstore)

Cryptographically verifiable transparency log used to record Tier-3 attack events.

Provides tamper-evident audit trails without blockchain overhead.

---

### React Dashboard

Real-time monitoring UI showing:

- active sessions
- threat scores
- attack patterns
- audit verification

---

# Communication Protocol

Internal service communication uses **gRPC** rather than REST.

Advantages:

- binary serialization
- lower latency
- strongly typed service contracts

This eliminates JSON parsing overhead in the critical request path.

---

# Request Flow


Client Request
↓
Go Gateway receives request
↓
Session state loaded from Redis
↓
Request sent via gRPC to Rust engine
↓
Behavioral signals computed
↓
Threat score evaluated
↓
Request forwarded to model
↓
Response optionally perturbed
↓
Response returned to client


Typical additional latency:


10–25 ms


which is negligible compared to most ML inference workloads.

---

# Tech Stack

| Component | Technology |
|---|---|
| Gateway | Go (Gin / Fiber) |
| Defense Engine | Rust (Axum / Tokio) |
| Embeddings | all-MiniLM-L6-v2 |
| Intent Classifier | cross-encoder/nli-MiniLM2-L6-H768 |
| Session Store | Redis |
| Analytics DB | PostgreSQL |
| Audit Log | Rekor (Sigstore) |
| Dashboard | React |
| Communication | gRPC |

---

# Threat Model

VYSE defends against:

- model extraction attacks
- gradient probing attacks
- adversarial boundary probing
- systematic API abuse
- explicit prompt-based extraction attempts

VYSE complements existing controls such as:

- WAFs
- API gateways
- rate limiters

It is not intended to replace them.

---

# Research Foundations

The design of VYSE is informed by prior research on ML model attacks and defenses.

Key references include:

Tramèr et al.  
**Stealing Machine Learning Models via Prediction APIs**  
USENIX Security 2016

Fredrikson et al.  
**Model Inversion Attacks that Exploit Confidence Information**  
ACM CCS 2015

Shokri et al.  
**Membership Inference Attacks Against Machine Learning Models**  
IEEE S&P 2017

Dwork et al.  
**The Algorithmic Foundations of Differential Privacy**

---
