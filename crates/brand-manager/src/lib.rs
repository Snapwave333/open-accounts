use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualIdentity {
    pub primary_hex: String,
    pub secondary_hex: String,
    pub accent_hex: String,
    pub font_family: String,
    pub logo_asset_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandVoice {
    pub tone_adjectives: Vec<String>, // e.g. ["Professional", "Innovative", "Empathetic"]
    pub audience_persona: String,
    pub writing_rules: Vec<String>,
    pub prohibited_words: Vec<String>,
    pub escalation_triggers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandProfile {
    pub brand_id: String,
    pub name: String,
    pub tagline: String,
    pub visual_identity: VisualIdentity,
    pub voice: BrandVoice,
    pub linked_account_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct BrandManager {
    brands: Arc<RwLock<HashMap<String, BrandProfile>>>,
}

impl BrandManager {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        let default_brand = BrandProfile {
            brand_id: "brand_default".into(),
            name: "OpenAccounts Global".into(),
            tagline: "Autonomous Agentic Account Infrastructure".into(),
            visual_identity: VisualIdentity {
                primary_hex: "#2563eb".into(),
                secondary_hex: "#0f172a".into(),
                accent_hex: "#10b981".into(),
                font_family: "Inter, sans-serif".into(),
                logo_asset_path: None,
            },
            voice: BrandVoice {
                tone_adjectives: vec!["Authoritative".into(), "Security-first".into(), "Helpful".into()],
                audience_persona: "Developers, founders, and social media automation managers.".into(),
                writing_rules: vec![
                    "Be concise, transparent, and direct.".into(),
                    "Highlight local-first privacy and deterministic compliance.".into(),
                ],
                prohibited_words: vec!["guaranteed riches".into(), "growth-hack-bot".into()],
                escalation_triggers: vec!["lawsuit".into(), "vulnerability".into(), "refund".into()],
            },
            linked_account_ids: vec!["acc-1".into(), "acc-5".into()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        map.insert(default_brand.brand_id.clone(), default_brand);

        Self {
            brands: Arc::new(RwLock::new(map)),
        }
    }

    pub async fn get_brand(&self, brand_id: &str) -> Option<BrandProfile> {
        self.brands.read().await.get(brand_id).cloned()
    }

    pub async fn list_brands(&self) -> Vec<BrandProfile> {
        self.brands.read().await.values().cloned().collect()
    }

    pub async fn create_brand(&self, profile: BrandProfile) -> String {
        let id = profile.brand_id.clone();
        self.brands.write().await.insert(id.clone(), profile);
        info!("Created new brand profile: {}", id);
        id
    }
}

impl Default for BrandManager {
    fn default() -> Self {
        Self::new()
    }
}
