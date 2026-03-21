pub mod logs;
pub mod session;

use bb8::Pool;
use bb8_redis::RedisConnectionManager;
use sqlx::PgPool;

/// Database connection pools passed through AppState to every handler.
#[derive(Clone)]
pub struct Stores {
    /// Redis connection pool — used for hot-path session state reads/writes.
    pub redis: Pool<RedisConnectionManager>,
    /// PostgreSQL connection pool — used for persistent query logs and audit records.
    pub postgres: PgPool,
}

impl Stores {
    /// Construct connection pools from URLs.
    pub async fn connect(
        redis_url: &str,
        postgres_url: &str,
        redis_pool_size: u32,
        postgres_max_connections: u32,
    ) -> Result<Self, crate::error::VyseError> {
        tracing::info!("connecting to Redis");
        let redis_manager = RedisConnectionManager::new(redis_url)?;
        let redis = Pool::builder()
            .max_size(redis_pool_size)
            .build(redis_manager)
            .await
            .map_err(|e| crate::error::VyseError::Internal(e.to_string()))?;

        tracing::info!("connecting to PostgreSQL");
        let postgres = sqlx::postgres::PgPoolOptions::new()
            .max_connections(postgres_max_connections)
            .connect(postgres_url)
            .await?;

        tracing::info!("running pending migrations");
        sqlx::migrate!("./migrations")
            .run(&postgres)
            .await?;

        Ok(Self { redis, postgres })
    }

    /// Run a quick health check on both stores.
    /// Returns (redis_ok, postgres_ok).
    pub async fn health_check(&self) -> (bool, bool) {
        let redis_ok = async {
            let mut conn = self.redis.get().await.ok()?;
            let pong: String = redis::cmd("PING")
                .query_async(&mut *conn)
                .await
                .ok()?;
            Some(pong == "PONG")
        }
        .await
        .unwrap_or(false);

        let postgres_ok = sqlx::query("SELECT 1")
            .execute(&self.postgres)
            .await
            .is_ok();

        (redis_ok, postgres_ok)
    }
}