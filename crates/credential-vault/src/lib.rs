use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result, SecretId};
use openaccounts_database::Database;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{info, warn};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const MIN_MASTER_PASSWORD_LEN: usize = 8;
const VERIFIER_PLAINTEXT: &[u8] = b"openaccounts-vault-verifier-v1";
const META_SALT: &str = "salt";
const META_VERIFIER: &str = "verifier";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadataView {
    pub id: SecretId,
    pub account_id: Option<String>,
    pub secret_type: SecretType,
    pub label: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
    pub persistent: bool,
    pub secret_count: usize,
    pub failed_attempts: u32,
}

type MasterKey = Zeroizing<[u8; 32]>;

fn derive_key(password: &str, salt: &[u8; 16]) -> Result<MasterKey> {
    let params = Params::new(65536, 3, 4, Some(32))
        .map_err(|e| OpenAccountsError::Vault(format!("Argon2 params error: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|e| OpenAccountsError::Vault(format!("Key derivation failed: {e}")))?;
    Ok(key)
}

fn encrypt(key: &MasterKey, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|e| OpenAccountsError::Vault(format!("Cipher init failed: {e}")))?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| OpenAccountsError::Vault(format!("Encryption error: {e}")))?;
    Ok((nonce_bytes.to_vec(), ciphertext))
}

fn decrypt(key: &MasterKey, nonce: &[u8], ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if nonce.len() != 12 {
        return Err(OpenAccountsError::Vault("Corrupted nonce".into()));
    }
    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|e| OpenAccountsError::Vault(format!("Cipher init failed: {e}")))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map(Zeroizing::new)
        .map_err(|_| OpenAccountsError::Vault("Decryption failed".into()))
}

fn encode_verifier(nonce: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nonce.len() + ciphertext.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(ciphertext);
    out
}

fn split_verifier(blob: &[u8]) -> Option<(&[u8], &[u8])> {
    (blob.len() > 12).then(|| blob.split_at(12))
}

fn db_err(e: sqlx::Error) -> OpenAccountsError {
    OpenAccountsError::Vault(format!("Vault storage error: {e}"))
}

pub struct VaultManager {
    master_key: Arc<RwLock<Option<MasterKey>>>,
    salt: RwLock<[u8; 16]>,
    verifier: RwLock<Option<Vec<u8>>>,
    secrets: Arc<RwLock<HashMap<SecretId, StoredSecretEntry>>>,
    store: Option<Arc<Database>>,
    failed_attempts: AtomicU32,
}

impl Default for VaultManager {
    fn default() -> Self {
        Self::new()
    }
}

impl VaultManager {
    pub fn new() -> Self {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        Self {
            master_key: Arc::new(RwLock::new(None)),
            salt: RwLock::new(salt),
            verifier: RwLock::new(None),
            secrets: Arc::new(RwLock::new(HashMap::new())),
            store: None,
            failed_attempts: AtomicU32::new(0),
        }
    }

    pub async fn with_database(db: Arc<Database>) -> Result<Self> {
        let pool = db.pool();
        let meta: HashMap<String, Vec<u8>> = sqlx::query("SELECT key, value FROM vault_meta")
            .fetch_all(pool)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(|r| (r.get::<String, _>("key"), r.get::<Vec<u8>, _>("value")))
            .collect();

        let salt = match meta.get(META_SALT).filter(|s| s.len() == 16) {
            Some(stored) => {
                let mut salt = [0u8; 16];
                salt.copy_from_slice(stored);
                salt
            }
            None => {
                let mut salt = [0u8; 16];
                OsRng.fill_bytes(&mut salt);
                sqlx::query("INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?, ?)")
                    .bind(META_SALT)
                    .bind(salt.to_vec())
                    .execute(pool)
                    .await
                    .map_err(db_err)?;
                salt
            }
        };

        let mut secrets = HashMap::new();
        let rows = sqlx::query(
            "SELECT id, account_id, secret_type, label, nonce, ciphertext, created_at FROM vault_secrets",
        )
        .fetch_all(pool)
        .await
        .map_err(db_err)?;
        for row in rows {
            let id_str: String = row.get("id");
            let Ok(uuid) = uuid::Uuid::parse_str(&id_str) else {
                warn!("Skipping vault entry with invalid id {}", id_str);
                continue;
            };
            let Ok(secret_type) = serde_json::from_str::<SecretType>(&row.get::<String, _>("secret_type")) else {
                warn!("Skipping vault entry {} with unknown type", id_str);
                continue;
            };
            let created_at = DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let id = SecretId(uuid);
            secrets.insert(
                id.clone(),
                StoredSecretEntry {
                    id,
                    account_id: row.get("account_id"),
                    secret_type,
                    label: row.get("label"),
                    nonce: row.get("nonce"),
                    ciphertext: row.get("ciphertext"),
                    created_at,
                },
            );
        }
        info!("Vault loaded with {} encrypted entries", secrets.len());

        Ok(Self {
            master_key: Arc::new(RwLock::new(None)),
            salt: RwLock::new(salt),
            verifier: RwLock::new(meta.get(META_VERIFIER).cloned()),
            secrets: Arc::new(RwLock::new(secrets)),
            store: Some(db),
            failed_attempts: AtomicU32::new(0),
        })
    }

    pub async fn is_unlocked(&self) -> bool {
        self.master_key.read().await.is_some()
    }

    pub async fn is_initialized(&self) -> bool {
        self.verifier.read().await.is_some()
    }

    pub async fn status(&self) -> VaultStatus {
        VaultStatus {
            initialized: self.is_initialized().await,
            unlocked: self.is_unlocked().await,
            persistent: self.store.is_some(),
            secret_count: self.secrets.read().await.len(),
            failed_attempts: self.failed_attempts.load(Ordering::SeqCst),
        }
    }

    async fn persist_meta(&self, key: &str, value: Vec<u8>) -> Result<()> {
        if let Some(db) = &self.store {
            sqlx::query("INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?, ?)")
                .bind(key)
                .bind(value)
                .execute(db.pool())
                .await
                .map_err(db_err)?;
        }
        Ok(())
    }

    pub async fn unlock(&self, master_password: &str) -> Result<()> {
        let salt = *self.salt.read().await;
        let existing = self.verifier.read().await.clone();

        if existing.is_none() && master_password.chars().count() < MIN_MASTER_PASSWORD_LEN {
            return Err(OpenAccountsError::Vault(format!(
                "Choose a master password of at least {MIN_MASTER_PASSWORD_LEN} characters"
            )));
        }

        let key = derive_key(master_password, &salt)?;

        match existing {
            Some(blob) => {
                let valid = split_verifier(&blob)
                    .map(|(nonce, ct)| {
                        decrypt(&key, nonce, ct).map(|p| p.as_slice() == VERIFIER_PLAINTEXT).unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !valid {
                    let attempts = self.failed_attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    let backoff = Duration::from_millis((250u64 << attempts.min(5)).min(8000));
                    warn!("Vault unlock rejected (attempt {})", attempts);
                    tokio::time::sleep(backoff).await;
                    return Err(OpenAccountsError::Vault("Incorrect master password".into()));
                }
            }
            None => {
                let (nonce, ct) = encrypt(&key, VERIFIER_PLAINTEXT)?;
                let blob = encode_verifier(&nonce, &ct);
                self.persist_meta(META_VERIFIER, blob.clone()).await?;
                *self.verifier.write().await = Some(blob);
                info!("Vault initialised with a new master password");
            }
        }

        self.failed_attempts.store(0, Ordering::SeqCst);
        *self.master_key.write().await = Some(key);
        Ok(())
    }

    pub async fn lock(&self) {
        self.master_key.write().await.take();
    }

    async fn current_key(&self) -> Result<MasterKey> {
        self.master_key
            .read()
            .await
            .as_ref()
            .map(|k| Zeroizing::new(**k))
            .ok_or_else(|| OpenAccountsError::Vault("Vault is locked".into()))
    }

    pub async fn store_secret(
        &self,
        account_id: Option<String>,
        secret_type: SecretType,
        label: String,
        payload: SecretPayload,
    ) -> Result<SecretId> {
        let key = self.current_key().await?;
        let serialized = Zeroizing::new(
            serde_json::to_vec(&payload).map_err(|e| OpenAccountsError::Vault(format!("Serialization error: {e}")))?,
        );
        let (nonce, ciphertext) = encrypt(&key, &serialized)?;

        let id = SecretId::new();
        let entry = StoredSecretEntry {
            id: id.clone(),
            account_id,
            secret_type,
            label,
            nonce,
            ciphertext,
            created_at: Utc::now(),
        };

        if let Some(db) = &self.store {
            sqlx::query(
                "INSERT INTO vault_secrets (id, account_id, secret_type, label, nonce, ciphertext, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(entry.id.0.to_string())
            .bind(&entry.account_id)
            .bind(serde_json::to_string(&entry.secret_type)?)
            .bind(&entry.label)
            .bind(&entry.nonce)
            .bind(&entry.ciphertext)
            .bind(entry.created_at.to_rfc3339())
            .execute(db.pool())
            .await
            .map_err(db_err)?;
        }

        self.secrets.write().await.insert(id.clone(), entry);
        Ok(id)
    }

    pub async fn get_secret(&self, id: &SecretId) -> Result<SecretPayload> {
        let key = self.current_key().await?;
        let secrets = self.secrets.read().await;
        let entry = secrets
            .get(id)
            .ok_or_else(|| OpenAccountsError::Vault("Secret not found".into()))?;
        let plaintext = decrypt(&key, &entry.nonce, &entry.ciphertext)?;
        serde_json::from_slice(&plaintext).map_err(|e| OpenAccountsError::Vault(format!("Deserialization error: {e}")))
    }

    pub async fn find_secret(&self, account_id: Option<&str>, label: &str) -> Option<SecretId> {
        self.secrets
            .read()
            .await
            .values()
            .filter(|e| e.label == label && e.account_id.as_deref() == account_id)
            .max_by_key(|e| e.created_at)
            .map(|e| e.id.clone())
    }

    pub async fn delete_secret(&self, id: &SecretId) -> Result<()> {
        self.current_key().await?;
        if let Some(db) = &self.store {
            sqlx::query("DELETE FROM vault_secrets WHERE id = ?")
                .bind(id.0.to_string())
                .execute(db.pool())
                .await
                .map_err(db_err)?;
        }
        self.secrets
            .write()
            .await
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| OpenAccountsError::Vault("Secret not found".into()))
    }

    pub async fn change_master_password(&self, current: &str, new_password: &str) -> Result<()> {
        if new_password.chars().count() < MIN_MASTER_PASSWORD_LEN {
            return Err(OpenAccountsError::Vault(format!(
                "Choose a master password of at least {MIN_MASTER_PASSWORD_LEN} characters"
            )));
        }
        self.unlock(current).await?;
        let old_key = self.current_key().await?;

        let mut new_salt = [0u8; 16];
        OsRng.fill_bytes(&mut new_salt);
        let new_key = derive_key(new_password, &new_salt)?;

        let mut secrets = self.secrets.write().await;
        let mut reencrypted = Vec::with_capacity(secrets.len());
        for entry in secrets.values() {
            let plaintext = decrypt(&old_key, &entry.nonce, &entry.ciphertext)?;
            let (nonce, ciphertext) = encrypt(&new_key, &plaintext)?;
            reencrypted.push((entry.id.clone(), nonce, ciphertext));
        }
        let (vn, vct) = encrypt(&new_key, VERIFIER_PLAINTEXT)?;
        let verifier = encode_verifier(&vn, &vct);

        if let Some(db) = &self.store {
            let mut tx = db.pool().begin().await.map_err(db_err)?;
            for (id, nonce, ct) in &reencrypted {
                sqlx::query("UPDATE vault_secrets SET nonce = ?, ciphertext = ? WHERE id = ?")
                    .bind(nonce)
                    .bind(ct)
                    .bind(id.0.to_string())
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            }
            for (key, value) in [(META_SALT, new_salt.to_vec()), (META_VERIFIER, verifier.clone())] {
                sqlx::query("INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?, ?)")
                    .bind(key)
                    .bind(value)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            }
            tx.commit().await.map_err(db_err)?;
        }

        for (id, nonce, ct) in reencrypted {
            if let Some(entry) = secrets.get_mut(&id) {
                entry.nonce = nonce;
                entry.ciphertext = ct;
            }
        }
        *self.salt.write().await = new_salt;
        *self.verifier.write().await = Some(verifier);
        *self.master_key.write().await = Some(new_key);
        info!("Vault master password rotated; {} entries re-encrypted", secrets.len());
        Ok(())
    }

    pub async fn list_metadata(&self) -> Vec<SecretMetadataView> {
        let mut out: Vec<SecretMetadataView> = self
            .secrets
            .read()
            .await
            .values()
            .map(|entry| SecretMetadataView {
                id: entry.id.clone(),
                account_id: entry.account_id.clone(),
                secret_type: entry.secret_type,
                label: entry.label.clone(),
                created_at: entry.created_at,
            })
            .collect();
        out.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        out
    }
}
