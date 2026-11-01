pub mod models;
pub mod preloaded;

pub use models::*;
pub use preloaded::get_preloaded_platforms;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;

pub struct PlatformIntelligence {
    profiles: Arc<RwLock<HashMap<String, PlatformKnowledgeProfile>>>,
}

impl PlatformIntelligence {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        for profile in get_preloaded_platforms() {
            map.insert(profile.id.clone(), profile);
        }
        info!("PlatformIntelligence loaded {} preloaded platform profiles", map.len());
        Self {
            profiles: Arc::new(RwLock::new(map)),
        }
    }

    pub async fn get_profile(&self, platform_id: &str) -> Option<PlatformKnowledgeProfile> {
        self.profiles.read().await.get(platform_id).cloned()
    }

    pub async fn list_profiles(&self) -> Vec<PlatformKnowledgeProfile> {
        self.profiles.read().await.values().cloned().collect()
    }

    pub async fn update_profile(&self, profile: PlatformKnowledgeProfile) {
        let mut lock = self.profiles.write().await;
        lock.insert(profile.id.clone(), profile);
    }
}

impl Default for PlatformIntelligence {
    fn default() -> Self {
        Self::new()
    }
}
