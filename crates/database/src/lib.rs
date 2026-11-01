use openaccounts_core::{OpenAccountsError, Result};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::path::Path;
use tracing::info;

pub type DbPool = Pool<Sqlite>;

pub struct Database {
    pool: DbPool,
}

impl Database {
    pub async fn init(database_path: &Path) -> Result<Self> {
        if let Some(parent) = database_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| OpenAccountsError::Database(format!("Failed to create db dir: {e}")))?;
        }

        let conn_str = format!("sqlite://{}?mode=rwc", database_path.to_string_lossy());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&conn_str)
            .await
            .map_err(|e| OpenAccountsError::Database(format!("Failed to connect to SQLite: {e}")))?;

        let db = Self { pool };
        db.run_migrations().await?;
        info!("Database initialized successfully at {:?}", database_path);
        Ok(db)
    }

    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                platform TEXT NOT NULL,
                username TEXT NOT NULL,
                email TEXT,
                phone TEXT,
                status TEXT NOT NULL,
                profile_metadata TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_active TEXT
            );

            CREATE TABLE IF NOT EXISTS automations (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                name TEXT NOT NULL,
                platform TEXT NOT NULL,
                module TEXT NOT NULL,
                configuration TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS audit_logs (
                id TEXT PRIMARY KEY,
                execution_id TEXT NOT NULL,
                account_id TEXT,
                automation_id TEXT,
                action_type TEXT NOT NULL,
                risk_level TEXT NOT NULL,
                details TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS human_tasks (
                id TEXT PRIMARY KEY,
                account_id TEXT,
                automation_id TEXT,
                task_type TEXT NOT NULL,
                reason TEXT NOT NULL,
                page_url TEXT,
                screenshot_path TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                resolved_at TEXT
            );

            CREATE TABLE IF NOT EXISTS lm_providers (
                id TEXT PRIMARY KEY,
                provider_type TEXT NOT NULL,
                name TEXT NOT NULL,
                endpoint TEXT,
                model_name TEXT NOT NULL,
                api_key_ref TEXT,
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenAccountsError::Database(format!("Migration failed: {e}")))?;

        Ok(())
    }
}
