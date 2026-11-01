use openaccounts_database::Database;
use openaccounts_lm_gateway::{LMGateway, ModelProfile, ModelProviderType};
use sqlx::Row;
use tracing::warn;

fn provider_key(p: &ModelProviderType) -> String {
    serde_json::to_value(p).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

pub async fn load_into(db: &Database, gateway: &LMGateway) -> Result<usize, String> {
    let rows = sqlx::query(
        "SELECT id, provider_type, name, endpoint, model_name, api_key_ref, max_tokens, temperature FROM lm_providers WHERE is_active = 1",
    )
    .fetch_all(db.pool())
    .await
    .map_err(|e| e.to_string())?;
    let mut loaded = 0;
    for row in rows {
        let id: String = row.get("id");
        let Ok(provider_type) = serde_json::from_value::<ModelProviderType>(serde_json::Value::String(row.get("provider_type"))) else {
            warn!("Skipping LM profile {} with unknown provider", id);
            continue;
        };
        gateway
            .register_profile(ModelProfile {
                id,
                name: row.get("name"),
                provider_type,
                endpoint: row.get("endpoint"),
                model_id: row.get("model_name"),
                api_key_secret_id: row.get("api_key_ref"),
                max_tokens: row.get::<Option<i64>, _>("max_tokens").map(|v| v.clamp(1, u32::MAX as i64) as u32),
                temperature: row.get::<Option<f64>, _>("temperature").map(|v| v as f32),
            })
            .await;
        loaded += 1;
    }
    Ok(loaded)
}

pub async fn save(db: &Database, profile: &ModelProfile) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO lm_providers (id, provider_type, name, endpoint, model_name, api_key_ref, is_active, created_at, max_tokens, temperature) \
         VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET provider_type = excluded.provider_type, name = excluded.name, endpoint = excluded.endpoint, \
         model_name = excluded.model_name, api_key_ref = excluded.api_key_ref, is_active = 1, \
         max_tokens = excluded.max_tokens, temperature = excluded.temperature",
    )
    .bind(&profile.id)
    .bind(provider_key(&profile.provider_type))
    .bind(&profile.name)
    .bind(&profile.endpoint)
    .bind(&profile.model_id)
    .bind(&profile.api_key_secret_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(profile.max_tokens.map(|v| v as i64))
    .bind(profile.temperature.map(|v| v as f64))
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn delete(db: &Database, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM lm_providers WHERE id = ?")
        .bind(id)
        .execute(db.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
