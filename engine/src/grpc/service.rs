use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::instrument;

use crate::{
    config::EngineConfig,
    error::VyseError,
    ml::Models,
    proto::{
        vyse_engine_server::VyseEngine,
        DeleteSessionRequest, DeleteSessionResponse, GetSessionRequest,
        GetStatsRequest, HealthCheckRequest, HealthCheckResponse,
        InferenceRequest, InferenceResponse, ListSessionsRequest,
        ScoreBreakdown as ProtoScoreBreakdown, SessionState as ProtoSessionState,
        StatsResponse,
    },
    scoring::{self, Tier},
    store::Stores,
};

/// Shared application state available to every gRPC handler.
#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<EngineConfig>,
    pub stores: Stores,
    pub models: Arc<Models>,
    pub start_time: std::time::Instant,
}

/// The gRPC service implementation.
pub struct VyseEngineService {
    state: AppState,
}

impl VyseEngineService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl VyseEngine for VyseEngineService {
    /// Handle a single inference request through the full defence pipeline.
    #[instrument(skip_all, fields(
        request_id = %req.get_ref().request_id,
        session = %&req.get_ref().session_id_hash[..8.min(req.get_ref().session_id_hash.len())],
    ))]
    async fn infer(
        &self,
        req: Request<InferenceRequest>,
    ) -> Result<Response<InferenceResponse>, Status> {
        let inner = req.into_inner();

        // Validate inputs.
        if inner.session_id_hash.is_empty() {
            return Err(Status::invalid_argument("session_id_hash is required"));
        }
        if inner.prompt.is_empty() {
            return Err(Status::invalid_argument("prompt is required"));
        }

        // Route through the inference pipeline.
        match self.handle_infer(inner).await {
            Ok(resp) => Ok(Response::new(resp)),
            Err(e) => Err(Status::from(e)),
        }
    }

    async fn get_session(
        &self,
        req: Request<GetSessionRequest>,
    ) -> Result<Response<ProtoSessionState>, Status> {
        let hash = &req.get_ref().session_id_hash;
        let mut redis = self
            .state
            .stores
            .redis
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        match crate::store::session::get(&mut *redis, hash).await {
            Ok(Some(session)) => Ok(Response::new(session_to_proto(session))),
            Ok(None) => Err(Status::not_found(format!("session not found: {hash}"))),
            Err(e) => Err(Status::from(e)),
        }
    }

    type ListSessionsStream = tokio_stream::wrappers::ReceiverStream<Result<ProtoSessionState, Status>>;

    async fn list_sessions(
        &self,
        req: Request<ListSessionsRequest>,
    ) -> Result<Response<Self::ListSessionsStream>, Status> {
        let filter = req.get_ref().tier_filter;
        let limit = req.get_ref().limit;

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let stores = self.state.stores.clone();

        tokio::spawn(async move {
            let mut redis = match stores.redis.get().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(e.to_string()))).await;
                    return;
                }
            };

            let hashes = match crate::store::session::list_hashes(&mut *redis).await {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx.send(Err(Status::from(e))).await;
                    return;
                }
            };

            let mut sent = 0i32;
            for hash in hashes {
                if limit > 0 && sent >= limit {
                    break;
                }
                if let Ok(Some(session)) = crate::store::session::get(&mut *redis, &hash).await {
                    if filter == 0 || session.tier == filter {
                        let proto = session_to_proto(session);
                        if tx.send(Ok(proto)).await.is_err() {
                            break; // client disconnected
                        }
                        sent += 1;
                    }
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn delete_session(
        &self,
        req: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let hash = &req.get_ref().session_id_hash;
        let mut redis = self
            .state
            .stores
            .redis
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        match crate::store::session::delete(&mut *redis, hash).await {
            Ok(true) => {
                tracing::info!(hash = %&hash[..8.min(hash.len())], "session deleted by admin");
                Ok(Response::new(DeleteSessionResponse {
                    success: true,
                    message: "session deleted".into(),
                }))
            }
            Ok(false) => Err(Status::not_found("session not found")),
            Err(e) => Err(Status::from(e)),
        }
    }

    async fn get_stats(
        &self,
        _req: Request<GetStatsRequest>,
    ) -> Result<Response<StatsResponse>, Status> {
        // Stats are aggregated from PostgreSQL query_logs.
        // Placeholder — full implementation in Phase 2.
        Ok(Response::new(StatsResponse::default()))
    }

    async fn health_check(
        &self,
        _req: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let (redis_ok, postgres_ok) = self.state.stores.health_check().await;
        let models_loaded = self.state.models.are_loaded();
        let rekor_ok = true; // TODO: ping Rekor in Phase 2

        let status = if redis_ok && postgres_ok && models_loaded {
            "healthy"
        } else if redis_ok || postgres_ok {
            "degraded"
        } else {
            "unhealthy"
        };

        Ok(Response::new(HealthCheckResponse {
            status: status.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            redis_connected: redis_ok,
            postgres_connected: postgres_ok,
            rekor_connected: rekor_ok,
            models_loaded,
            uptime_seconds: self.state.start_time.elapsed().as_secs_f32(),
        }))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Inference pipeline (private)
// ─────────────────────────────────────────────────────────────────────────────

impl VyseEngineService {
    async fn handle_infer(&self, req: InferenceRequest) -> Result<InferenceResponse, VyseError> {
        let cfg = &self.state.cfg;
        let now = chrono::Utc::now();

        // ── 1. Load or create session ─────────────────────────────────────────
        let mut redis = self.state.stores.redis.get().await
            .map_err(|e| VyseError::Internal(e.to_string()))?;

        let mut session =
            crate::store::session::get(&mut *redis, &req.session_id_hash)
                .await?
                .unwrap_or_else(|| crate::store::session::Session::new(&req.session_id_hash));

        if session.is_banned {
            return Err(VyseError::SessionBanned {
                session_hash: req.session_id_hash.clone(),
            });
        }

        // ── 2. Update timestamps and prompt history ───────────────────────────
        session.timestamps.push(now);
        session.prompt_history.push(req.prompt.clone());
        session.request_count += 1;
        session.last_active_at = now;
        session.trim(cfg.scoring.max_timestamps, cfg.scoring.entropy_window * 2);

        // ── 3. Compute embedding for current prompt ───────────────────────────
        let embedding = self.state.models.embed(&req.prompt).await?;

        // ── 4. Run intent classification ──────────────────────────────────────
        let (intent_flagged, intent_label) = self
            .state
            .models
            .classify_intent(&req.prompt, &cfg.ml.intent_labels, cfg.ml.intent_threshold)
            .await?;

        // ── 5. Compute scores ─────────────────────────────────────────────────
        let prev_embedding = session.last_embedding.as_deref();
        let scoring_ctx = scoring::ScoringContext {
            prompt: &req.prompt,
            prompt_embedding: &embedding,
            prev_embedding,
            timestamps: &session.timestamps,
            prompt_history: &session.prompt_history,
            duration_mins: session.tracking_duration_mins(),
        };
        let mut scores = scoring::compute_scores(&scoring_ctx, &cfg.scoring);
        scores.intent_flagged = intent_flagged;
        scores.intent_label = intent_label;

        // ── 6. Start tracking if threshold crossed ────────────────────────────
        if scores.hybrid > cfg.scoring.tracking_threshold && session.tracking_started_at.is_none() {
            session.tracking_started_at = Some(now);
            tracing::info!(
                hash = %&req.session_id_hash[..8],
                score = scores.hybrid,
                "tracking started"
            );
        }

        let duration_mins = session.tracking_duration_mins();

        // ── 7. Classify tier ──────────────────────────────────────────────────
        let tier = scoring::classify_tier(&scores, duration_mins, &cfg.scoring);
        session.tier = tier.as_i32();
        session.current_hybrid_score = scores.hybrid;
        session.dynamic_mean_rpm = scoring::velocity::calculate_rpm(
            &session.timestamps,
            cfg.scoring.velocity_window_mins,
        );
        session.last_embedding = Some(embedding);

        // ── 8. Get LLM response ───────────────────────────────────────────────
        let clean_response = self.get_llm_response(&req.prompt).await?;

        // ── 9. Apply defence ──────────────────────────────────────────────────
        let (served_response, noise_applied) = match tier {
            Tier::Clean => (clean_response.clone(), false),
            Tier::Suspicious | Tier::Malicious => {
                // Derive a deterministic seed so perturbation is reproducible.
                let seed = session.noise_seed.get_or_insert_with(|| {
                    crate::defence::derive_seed(
                        &req.session_id_hash,
                        session.tracking_started_at.as_ref().unwrap_or(&now),
                    )
                }).clone();

                let perturbed = crate::defence::apply(
                    &clean_response,
                    tier,
                    &seed,
                    &cfg.defence,
                )?;
                (perturbed, true)
            }
        };

        // ── 10. Tier 3: submit to Rekor ───────────────────────────────────────
        if tier == Tier::Malicious {
            let rekor_client = crate::audit::RekorClient::new(cfg.rekor.clone());
            tokio::spawn(async move {
                let _ = rekor_client.submit_tier3_event(
                    &req.session_id_hash,
                    scores.hybrid,
                    duration_mins,
                    session.noise_seed.as_deref().unwrap_or(""),
                ).await;
            });
        }

        // ── 11. Persist session + log (async, non-blocking) ───────────────────
        let ttl = cfg.redis.session_ttl_secs;
        {
            let session_clone = session.clone();
            let mut redis_clone = self.state.stores.redis.get().await
                .map_err(|e| VyseError::Internal(e.to_string()))?;
            tokio::spawn(async move {
                let _ = crate::store::session::set(&mut *redis_clone, &session_clone, ttl).await;
            });
        }

        let log = crate::store::logs::QueryLog {
            id: uuid::Uuid::new_v4(),
            session_id_hash: req.session_id_hash.clone(),
            timestamp: now,
            prompt_hash: crate::defence::hash_prompt(&req.prompt),
            tier: tier.as_i32(),
            hybrid_score: scores.hybrid,
            v_score: scores.v_score,
            d_score: scores.d_score,
            e_score: scores.e_score,
            a_score: scores.a_score,
            intent_flagged: scores.intent_flagged,
            intent_label: scores.intent_label.clone(),
            noise_applied,
            noise_steps: None,
            request_id: req.request_id.clone(),
        };
        {
            let pool = self.state.stores.postgres.clone();
            tokio::spawn(async move {
                let _ = crate::store::logs::insert(&pool, &log).await;
            });
        }

        Ok(InferenceResponse {
            response: served_response,
            tier: tier.as_i32(),
            hybrid_score: scores.hybrid,
            duration_mins,
            request_id: req.request_id,
            score_breakdown: Some(ProtoScoreBreakdown {
                v_score: scores.v_score,
                d_score: scores.d_score,
                e_score: scores.e_score,
                a_score: scores.a_score,
                intent_flagged: scores.intent_flagged,
                intent_label: scores.intent_label.unwrap_or_default(),
            }),
        })
    }

    async fn get_llm_response(&self, prompt: &str) -> Result<String, VyseError> {
        // LLM provider interface — dispatches to Groq/OpenAI/Ollama based on config.
        // Full implementation in ml/llm.rs
        crate::ml::llm::complete(prompt, &self.state.cfg.llm).await
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversion helpers
// ─────────────────────────────────────────────────────────────────────────────

fn session_to_proto(s: crate::store::session::Session) -> ProtoSessionState {
    ProtoSessionState {
        session_id_hash: s.session_id_hash,
        tier: s.tier,
        first_seen_at: s.first_seen_at.timestamp(),
        last_active_at: s.last_active_at.timestamp(),
        tracking_started_at: s.tracking_started_at.map(|t| t.timestamp()).unwrap_or(0),
        request_count: s.request_count as i64,
        dynamic_mean_rpm: s.dynamic_mean_rpm,
        current_hybrid_score: s.current_hybrid_score,
        duration_mins: s.tracking_duration_mins(),
        is_banned: s.is_banned,
    }
}