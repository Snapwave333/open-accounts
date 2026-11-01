use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use openaccounts_core::{OpenAccountsError, Result, SecretId};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Serialize, Deserialize)]
pub enum SecretType {
    Password,
    ApiKey,
    OAuthToken,
    SessionCookie,
    RecoveryCode,
    CustomNote,
}

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct SecretPayload {
    pub value: String,
    pub metadata: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredSecretEntry {
    pub id: SecretId,
    pub account_id: Option<String>,
    pub secret_type: SecretType,
    pub label: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct VaultManager {
    master_key: Arc<RwLock<Option<[u8; 32]>>>,
    salt: [u8; 16],
    secrets: Arc<RwLock<HashMap<SecretId, StoredSecretEntry>>>,
}

impl VaultManager {
    pub fn new() -> Self {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        Self {
            master_key: Arc::new(RwLock::new(None)),
            salt,
            secrets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn is_unlocked(&self) -> bool {
        self.master_key.read().await.is_some()
    }

    pub async fn unlock(&self, master_password: &str) -> Result<()> {
        let params = Params::new(65536, 3, 4, Some(32))
            .map_err(|e| OpenAccountsError::Vault(format!("Argon2 params error: {e}")))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(master_password.as_bytes(), &self.salt, &mut key)
            .map_err(|e| OpenAccountsError::Vault(format!("Key derivation failed: {e}")))?;

        let mut lock = self.master_key.write().await;
        *lock = Some(key);
        Ok(())
    }

    pub async fn lock(&self) {
        let mut lock = self.master_key.write().await;
        if let Some(mut key) = lock.take() {
            key.zeroize();
        }
    }

    pub async fn store_secret(
        &self,
        account_id: Option<String>,
        secret_type: SecretType,
        label: String,
        payload: SecretPayload,
    ) -> Result<SecretId> {
        let key_opt = *self.master_key.read().await;
        let key = key_opt.ok_or_else(|| OpenAccountsError::Vault("Vault is locked".into()))?;

        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| OpenAccountsError::Vault(format!("Cipher init failed: {e}")))?;

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let serialized_payload = serde_json::to_vec(&payload)
            .map_err(|e| OpenAccountsError::Vault(format!("Serialization error: {e}")))?;

        let ciphertext = cipher
            .encrypt(nonce, serialized_payload.as_ref())
            .map_err(|e| OpenAccountsError::Vault(format!("Encryption error: {e}")))?;

        let id = SecretId::new();
        let entry = StoredSecretEntry {
            id: id.clone(),
            account_id,
            secret_type,
            label,
            nonce: nonce_bytes.to_vec(),
            ciphertext,
            created_at: chrono::Utc::now(),
        };

        let mut secrets = self.secrets.write().await;
        secrets.insert(id.clone(), entry);

        Ok(id)
    }

    pub async fn get_secret(&self, id: &SecretId) -> Result<SecretPayload> {
        let key_opt = *self.master_key.read().await;
        let key = key_opt.ok_or_else(|| OpenAccountsError::Vault("Vault is locked".into()))?;

        let secrets = self.secrets.read().await;
        let entry = secrets
            .get(id)
            .ok_or_else(|| OpenAccountsError::Vault("Secret not found".into()))?;

        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| OpenAccountsError::Vault(format!("Cipher init failed: {e}")))?;

        let nonce = Nonce::from_slice(&entry.nonce);
        let plaintext_bytes = cipher
            .decrypt(nonce, entry.ciphertext.as_ref())
            .map_err(|e| OpenAccountsError::Vault(format!("Decryption error: {e}")))?;

        let payload: SecretPayload = serde_json::from_slice(&plaintext_bytes)
            .map_err(|e| OpenAccountsError::Vault(format!("Deserialization error: {e}")))?;

        Ok(payload)
    }

    pub async fn list_metadata(&self) -> Vec<SecretMetadataView> {
        let secrets = self.secrets.read().await;
        secrets
            .values()
            .map(|entry| SecretMetadataView {
                id: entry.id.clone(),
                account_id: entry.account_id.clone(),
                secret_type: entry.secret_type.clone(),
                label: entry.label.clone(),
                created_at: entry.created_at,
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize)]
pub struct SecretMetadataView {
    pub id: SecretId,
    pub account_id: Option<String>,
    pub secret_type: SecretType,
    pub label: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
