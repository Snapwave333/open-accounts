use thiserror::Error;

#[derive(Error, Debug)]
pub enum OpenAccountsError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Vault error: {0}")]
    Vault(String),

    #[error("LM Gateway error: {0}")]
    LMGateway(String),

    #[error("Browser automation error: {0}")]
    Browser(String),

    #[error("Human intervention required: {0}")]
    HumanInterventionRequired(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Rate limit exceeded for account {0}: retry after {1}s")]
    RateLimitExceeded(String, u64),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Automation error: {0}")]
    Automation(String),

    #[error("Kill switch activated: operation aborted")]
    KillSwitchActivated,

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, OpenAccountsError>;
