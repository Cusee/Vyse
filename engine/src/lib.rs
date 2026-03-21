//! `vyse-engine` — the Rust defence engine for the Vyse ML API security layer.
//!
//! # Module layout
//!
//! - [`config`]   — load and validate all configuration
//! - [`scoring`]  — compute V/D/E/A scores and classify tiers
//! - [`defence`]  — apply perturbation to responses
//! - [`store`]    — session state (Redis) and persistent logs (PostgreSQL)
//! - [`audit`]    — Rekor transparency log submissions
//! - [`llm`]      — LLM provider abstraction (Groq, OpenAI, Ollama)
//! - [`grpc`]     — tonic gRPC service implementation
//! - [`proto`]    — proto-generated types (included from OUT_DIR)

pub mod audit;
pub mod config;
pub mod defence;
pub mod grpc;
pub mod llm;
pub mod proto;
pub mod scoring;
pub mod store;