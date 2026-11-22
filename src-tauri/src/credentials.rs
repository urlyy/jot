//! System keychain access for API keys.
//!
//! Secrets never live in `settings.json`; they are stored in the OS vault via
//! the `keyring` crate. The transcription key uses namespace `stt` with
//! provider `custom` (OpenAI-compatible) or `volcengine`; the polishing key
//! uses namespace `llm` with provider `custom`.

use anyhow::{anyhow, Context, Result};
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};

const SERVICE_NAME: &str = "Jot";
const API_KEY_ACCOUNT_SUFFIX: &str = "api_key";
const STORED_CREDENTIAL_VERSION: u8 = 1;

/// Credential namespace for the transcription endpoint.
pub const STT_NAMESPACE: &str = "stt";
/// Credential namespace for the polishing endpoint.
pub const LLM_NAMESPACE: &str = "llm";
/// The provider identifier used by the OpenAI-compatible endpoint.
pub const CUSTOM_PROVIDER: &str = "custom";
/// The provider identifier used by the Volcengine streaming endpoint.
pub const VOLCENGINE_PROVIDER: &str = crate::storage::STT_PROVIDER_VOLCENGINE;

pub trait CredentialVault {
    fn set_secret(&self, namespace: &str, provider: &str, secret: &str) -> Result<()>;
}

pub trait CredentialSecretReader {
    fn get_secret(&self, namespace: &str, provider: &str) -> Result<Option<String>>;

    fn get_secret_updated_at(&self, _namespace: &str, _provider: &str) -> Result<Option<String>> {
        Ok(None)
    }
}

pub trait CredentialSecretRemover {
    fn remove_secret(&self, namespace: &str, provider: &str) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCredential {
    pub value: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCredentialPayload {
    version: u8,
    secret_kind: String,
    value: String,
    updated_at: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCredentialVault;

impl SystemCredentialVault {
    fn account(namespace: &str, provider: &str) -> String {
        format!(
            "{}.{}.{}",
            namespace.trim(),
            provider.trim(),
            API_KEY_ACCOUNT_SUFFIX
        )
    }

    pub fn has_secret(&self, namespace: &str, provider: &str) -> Result<bool> {
        Ok(self
            .get_secret(namespace, provider)?
            .is_some_and(|secret| !secret.trim().is_empty()))
    }

    pub fn remove_secret(&self, namespace: &str, provider: &str) -> Result<()> {
        delete_system_secret(&Self::account(namespace, provider))
    }
}

impl CredentialVault for SystemCredentialVault {
    fn set_secret(&self, namespace: &str, provider: &str, secret: &str) -> Result<()> {
        let stored = encode_stored_credential(secret, &current_credential_timestamp())
            .map_err(|error| anyhow!("编码钥匙串内容失败（{namespace}.{provider}）：{error}"))?;
        write_system_secret(&Self::account(namespace, provider), &stored)
    }
}

impl CredentialSecretReader for SystemCredentialVault {
    fn get_secret(&self, namespace: &str, provider: &str) -> Result<Option<String>> {
        read_system_secret(&Self::account(namespace, provider))?
            .map(|stored| decode_stored_credential(&stored).map(|credential| credential.value))
            .transpose()
    }

    fn get_secret_updated_at(&self, namespace: &str, provider: &str) -> Result<Option<String>> {
        read_system_secret(&Self::account(namespace, provider))?
            .map(|stored| decode_stored_credential(&stored).map(|credential| credential.updated_at))
            .transpose()
            .map(Option::flatten)
    }
}

impl CredentialSecretRemover for SystemCredentialVault {
    fn remove_secret(&self, namespace: &str, provider: &str) -> Result<()> {
        Self::remove_secret(self, namespace, provider)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn system_entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE_NAME, account).context("打开系统钥匙串失败")
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn write_system_secret(account: &str, secret: &str) -> Result<()> {
    system_entry(account)?
        .set_password(secret)
        .with_context(|| format!("写入系统钥匙串失败：{account}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn write_system_secret(_account: &str, _secret: &str) -> Result<()> {
    Err(anyhow!("当前平台不支持系统钥匙串"))
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn read_system_secret(account: &str) -> Result<Option<String>> {
    match system_entry(account)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(anyhow!("读取系统钥匙串失败（{account}）：{error}")),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn read_system_secret(_account: &str) -> Result<Option<String>> {
    Err(anyhow!("当前平台不支持系统钥匙串"))
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn delete_system_secret(account: &str) -> Result<()> {
    match system_entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(anyhow!("删除系统钥匙串条目失败（{account}）：{error}")),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn delete_system_secret(_account: &str) -> Result<()> {
    Err(anyhow!("当前平台不支持系统钥匙串"))
}

fn current_credential_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn encode_stored_credential(secret: &str, updated_at: &str) -> Result<String> {
    serde_json::to_string(&StoredCredentialPayload {
        version: STORED_CREDENTIAL_VERSION,
        secret_kind: "apiKey".to_string(),
        value: secret.to_string(),
        updated_at: updated_at.to_string(),
    })
    .context("序列化钥匙串内容失败")
}

fn decode_stored_credential(stored: &str) -> Result<StoredCredential> {
    match serde_json::from_str::<StoredCredentialPayload>(stored) {
        Ok(payload) if payload.version == STORED_CREDENTIAL_VERSION => Ok(StoredCredential {
            value: payload.value,
            updated_at: Some(payload.updated_at),
        }),
        _ => Ok(StoredCredential {
            value: stored.to_string(),
            updated_at: None,
        }),
    }
}

/// Read a secret from the vault, mapping "not configured" to an empty string.
pub fn read_secret<V: CredentialSecretReader>(
    vault: &V,
    namespace: &str,
    provider: &str,
) -> Result<String> {
    Ok(vault
        .get_secret(namespace, provider)?
        .unwrap_or_default())
}

/// Read the transcription API key (namespace `stt`, provider `custom`).
pub fn read_stt_secret<V: CredentialSecretReader>(vault: &V) -> Result<String> {
    read_secret(vault, STT_NAMESPACE, CUSTOM_PROVIDER)
}

/// Read the transcription API key for a specific STT provider.
pub fn read_stt_secret_for_provider<V: CredentialSecretReader>(
    vault: &V,
    provider: &str,
) -> Result<String> {
    read_secret(vault, STT_NAMESPACE, provider)
}

/// Read the Volcengine transcription API key (namespace `stt`, provider
/// `volcengine`). It is stored separately from the OpenAI-compatible key.
pub fn read_volcengine_secret<V: CredentialSecretReader>(vault: &V) -> Result<String> {
    read_secret(vault, STT_NAMESPACE, VOLCENGINE_PROVIDER)
}

/// Read the polishing API key (namespace `llm`, provider `custom`).
pub fn read_llm_secret<V: CredentialSecretReader>(vault: &V) -> Result<String> {
    read_secret(vault, LLM_NAMESPACE, CUSTOM_PROVIDER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryVault {
        records: Mutex<HashMap<(String, String), String>>,
    }

    impl CredentialVault for MemoryVault {
        fn set_secret(&self, namespace: &str, provider: &str, secret: &str) -> Result<()> {
            self.records.lock().unwrap().insert(
                (namespace.to_string(), provider.to_string()),
                secret.to_string(),
            );
            Ok(())
        }
    }

    impl CredentialSecretReader for MemoryVault {
        fn get_secret(&self, namespace: &str, provider: &str) -> Result<Option<String>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .get(&(namespace.to_string(), provider.to_string()))
                .cloned())
        }
    }

    impl CredentialSecretRemover for MemoryVault {
        fn remove_secret(&self, namespace: &str, provider: &str) -> Result<()> {
            self.records
                .lock()
                .unwrap()
                .remove(&(namespace.to_string(), provider.to_string()));
            Ok(())
        }
    }

    #[test]
    fn reads_stt_and_llm_secrets_from_their_namespaces() {
        let vault = MemoryVault::default();
        vault.set_secret(STT_NAMESPACE, CUSTOM_PROVIDER, "stt-key").unwrap();
        vault.set_secret(LLM_NAMESPACE, CUSTOM_PROVIDER, "llm-key").unwrap();

        assert_eq!(read_stt_secret(&vault).unwrap(), "stt-key");
        assert_eq!(read_llm_secret(&vault).unwrap(), "llm-key");
    }

    #[test]
    fn missing_secret_reads_as_empty_string() {
        let vault = MemoryVault::default();
        assert_eq!(read_stt_secret(&vault).unwrap(), "");
    }

    #[test]
    fn volcengine_key_is_stored_separately_from_the_openai_compatible_key() {
        let vault = MemoryVault::default();
        vault.set_secret(STT_NAMESPACE, CUSTOM_PROVIDER, "openai-key").unwrap();
        vault
            .set_secret(STT_NAMESPACE, VOLCENGINE_PROVIDER, "volc-key")
            .unwrap();

        assert_eq!(read_stt_secret(&vault).unwrap(), "openai-key");
        assert_eq!(read_volcengine_secret(&vault).unwrap(), "volc-key");
        assert_eq!(
            read_stt_secret_for_provider(&vault, VOLCENGINE_PROVIDER).unwrap(),
            "volc-key"
        );
    }

    #[test]
    fn stored_credential_payload_round_trips_secret_and_metadata() {
        let stored = encode_stored_credential("vault-secret", "2026-07-06T00:00:00Z").unwrap();
        assert_ne!(stored, "vault-secret");

        let decoded = decode_stored_credential(&stored).unwrap();
        assert_eq!(decoded.value, "vault-secret");
        assert_eq!(decoded.updated_at.as_deref(), Some("2026-07-06T00:00:00Z"));
    }

    #[test]
    fn stored_credential_decoder_preserves_legacy_plaintext_secret() {
        let decoded = decode_stored_credential("legacy-secret").unwrap();
        assert_eq!(decoded.value, "legacy-secret");
        assert_eq!(decoded.updated_at, None);
    }
}
