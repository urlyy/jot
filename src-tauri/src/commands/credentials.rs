//! Keychain-backed credential commands (namespace `stt` / `llm`).
//!
//! The transcription namespace accepts provider `custom` (OpenAI-compatible)
//! and `volcengine`; the polishing namespace uses provider `custom`.

use serde::Serialize;

use crate::credentials::{
    CredentialSecretReader, CredentialVault, SystemCredentialVault, CUSTOM_PROVIDER, LLM_NAMESPACE,
    STT_NAMESPACE,
};

/// Contract shape of `get_credential_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CredentialStatus {
    pub stt: bool,
    pub llm: bool,
}

pub(crate) fn validate_credential_target(
    namespace: &str,
    provider: &str,
) -> Result<(String, String), String> {
    let namespace = namespace.trim();
    let provider = provider.trim();

    if namespace != STT_NAMESPACE && namespace != LLM_NAMESPACE {
        return Err(format!(
            "未知的密钥命名空间「{namespace}」，仅支持 {STT_NAMESPACE} 与 {LLM_NAMESPACE}"
        ));
    }
    if provider.is_empty() {
        return Err("密钥的 provider 不能为空".to_string());
    }
    let provider_valid = provider.len() <= 80
        && provider
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'));
    if !provider_valid {
        return Err(format!("不支持的 provider「{provider}」"));
    }

    Ok((namespace.to_string(), provider.to_string()))
}

fn status_from_vault<V: CredentialSecretReader>(vault: &V) -> CredentialStatus {
    let check = |namespace: &str| match vault.get_secret(namespace, CUSTOM_PROVIDER) {
        Ok(secret) => secret.is_some_and(|value| !value.trim().is_empty()),
        Err(error) => {
            tracing::warn!("读取系统钥匙串失败（{namespace}）：{error}");
            false
        }
    };

    CredentialStatus {
        stt: check(STT_NAMESPACE),
        llm: check(LLM_NAMESPACE),
    }
}

#[tauri::command]
pub fn read_credential(namespace: String, provider: String) -> Result<Option<String>, String> {
    let (namespace, provider) = validate_credential_target(&namespace, &provider)?;
    SystemCredentialVault
        .get_secret(&namespace, &provider)
        .map_err(|error| format!("读取密钥失败：{error}"))
}

#[tauri::command]
pub fn set_credential(namespace: String, provider: String, secret: String) -> Result<(), String> {
    let (namespace, provider) = validate_credential_target(&namespace, &provider)?;
    let vault = SystemCredentialVault;

    if secret.trim().is_empty() {
        vault
            .remove_secret(&namespace, &provider)
            .map_err(|error| format!("清除密钥失败：{error}"))
    } else {
        vault
            .set_secret(&namespace, &provider, &secret)
            .map_err(|error| format!("保存密钥失败：{error}"))
    }
}

#[tauri::command]
pub fn clear_credential(namespace: String, provider: String) -> Result<(), String> {
    let (namespace, provider) = validate_credential_target(&namespace, &provider)?;
    SystemCredentialVault
        .remove_secret(&namespace, &provider)
        .map_err(|error| format!("清除密钥失败：{error}"))
}

#[tauri::command]
pub fn get_credential_status() -> Result<CredentialStatus, String> {
    Ok(status_from_vault(&SystemCredentialVault))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::CredentialSecretRemover;
    use anyhow::Result;
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
    fn accepts_stt_and_llm_namespaces_with_custom_provider() {
        assert_eq!(
            validate_credential_target(" stt ", " custom ").unwrap(),
            ("stt".to_string(), "custom".to_string())
        );
        assert_eq!(
            validate_credential_target("llm", "custom").unwrap(),
            ("llm".to_string(), "custom".to_string())
        );
    }

    #[test]
    fn rejects_unknown_namespace_and_unsafe_provider() {
        assert!(validate_credential_target("session", "custom")
            .unwrap_err()
            .contains("未知的密钥命名空间"));
        assert!(validate_credential_target("llm", "custom/key")
            .unwrap_err()
            .contains("不支持的 provider"));
        assert!(validate_credential_target("llm", " ")
            .unwrap_err()
            .contains("不能为空"));
    }

    #[test]
    fn status_reports_missing_and_present_keys() {
        let vault = MemoryVault::default();
        assert_eq!(
            status_from_vault(&vault),
            CredentialStatus {
                stt: false,
                llm: false
            }
        );

        vault.set_secret(STT_NAMESPACE, CUSTOM_PROVIDER, "stt-key").unwrap();
        vault.set_secret(LLM_NAMESPACE, CUSTOM_PROVIDER, "llm-key").unwrap();
        assert_eq!(
            status_from_vault(&vault),
            CredentialStatus {
                stt: true,
                llm: true
            }
        );
    }
}
