use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub Uuid);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AutomationId(pub Uuid);

impl AutomationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AutomationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AutomationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretId(pub Uuid);

impl SecretId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SecretId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SecretId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", from = "String")]
pub enum Platform {
    Instagram,
    Reddit,
    YouTube,
    Discord,
    X,
    Email,
    Custom(String),
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Instagram => write!(f, "instagram"),
            Platform::Reddit => write!(f, "reddit"),
            Platform::YouTube => write!(f, "youtube"),
            Platform::Discord => write!(f, "discord"),
            Platform::X => write!(f, "x"),
            Platform::Email => write!(f, "email"),
            Platform::Custom(s) => write!(f, "{}", s),
        }
    }
}

impl From<&str> for Platform {
    fn from(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "instagram" => Platform::Instagram,
            "reddit" => Platform::Reddit,
            "youtube" => Platform::YouTube,
            "discord" => Platform::Discord,
            "x" | "twitter" => Platform::X,
            "email" => Platform::Email,
            other => Platform::Custom(other.to_string()),
        }
    }
}

impl From<String> for Platform {
    fn from(value: String) -> Self {
        Platform::from(value.as_str())
    }
}

impl From<Platform> for String {
    fn from(value: Platform) -> Self {
        value.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    ReadComments,
    DraftResponse,
    PublishResponse,
    DeleteComment,
    FollowUser,
    UnfollowUser,
    LikePost,
    CreatePost,
    ChangePassword,
    ExportCredentials,
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Pending,
    Running,
    PausedAwaitingHuman,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub total_accounts: u64,
    pub active_automations: u64,
    pub pending_human_tasks: u64,
    pub system_status: String,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::Platform;

    #[test]
    fn platform_serializes_as_plain_string() {
        assert_eq!(serde_json::to_string(&Platform::Custom("tiktok".into())).unwrap(), "\"tiktok\"");
        assert_eq!(serde_json::to_string(&Platform::YouTube).unwrap(), "\"youtube\"");
        let parsed: Platform = serde_json::from_str("\"Twitter\"").unwrap();
        assert_eq!(parsed, Platform::X);
        let legacy: Platform = serde_json::from_str("\"shopify\"").unwrap();
        assert_eq!(legacy, Platform::Custom("shopify".into()));
    }
}
