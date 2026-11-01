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
    pub tone_adjectives: Vec<String>,
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
        let cdn_brand = BrandProfile {
            brand_id: "brand_cdn".into(),
            name: "Clearly Drawn News (StickExplainers)".into(),
            tagline: "Daily Animation News + Cartoon Kitchen with Marcus & Chef Elena".into(),
            visual_identity: VisualIdentity {
                primary_hex: "#141722".into(),
                secondary_hex: "#0b0d12".into(),
                accent_hex: "#d946ef".into(),
                font_family: "Inter, sans-serif".into(),
                logo_asset_path: Some(r"C:\Users\chrom\Desktop\StickExplainers\assets\cdn_logo.png".into()),
            },
            voice: BrandVoice {
                tone_adjectives: vec![
                    "Upbeat".into(),
                    "Warm".into(),
                    "Family-Friendly".into(),
                    "Fact-Checked".into(),
                ],
                audience_persona: "Anime, cartoon, and animation fans of all ages + food lovers watching Cartoon Kitchen.".into(),
                writing_rules: vec![
                    "Write in character as anchor Marcus or Chef Elena.".into(),
                    "Credit every news outlet and recipe creator explicitly.".into(),
                    "Max 5 hashtags per feed post; at most 2 feed posts/day, 4+ hours apart.".into(),
                    "Never post links in comments except our Shopify merch store on YouTube.".into(),
                ],
                prohibited_words: vec![
                    "nsfw".into(),
                    "follow for follow".into(),
                    "f4f".into(),
                    "crypto".into(),
                    "giveaway".into(),
                ],
                escalation_triggers: vec![
                    "personal info".into(),
                    "kids".into(),
                    "action blocked".into(),
                    "sponsor".into(),
                    "copyright".into(),
                ],
            },
            linked_account_ids: vec![
                "cdn-ig-snapwave".into(),
                "cdn-tt-clearlydrawnlessons".into(),
                "cdn-yt-clearlydrawnnews".into(),
                "cdn-shopify-store".into(),
                "cdn-paypal-snapwave333".into(),
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        map.insert(cdn_brand.brand_id.clone(), cdn_brand);

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
