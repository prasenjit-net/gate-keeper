use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::Identity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use x509_parser::pem::parse_x509_pem;

use crate::error::{AppError, AppResult};

const CERT_FILE_NAME: &str = "client.crt";
const KEY_FILE_NAME: &str = "client.key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateConfig {
    pub id: String,
    pub name: String,
    pub hosts: Vec<String>,
    pub enabled: bool,
    pub cert_path: String,
    pub key_path: String,
    pub cert_file_name: String,
    pub key_file_name: String,
    #[serde(default)]
    pub subject_distinguished_name: String,
    #[serde(default)]
    pub issuer_distinguished_name: String,
    #[serde(default)]
    pub serial_number: String,
    #[serde(default)]
    pub valid_from: String,
    #[serde(default)]
    pub valid_until: String,
    pub fingerprint_sha256: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateMatch {
    pub id: String,
    pub name: String,
    pub matched_host_pattern: String,
}

#[derive(Debug)]
pub struct CertificateUpload {
    pub name: String,
    pub hosts: Vec<String>,
    pub enabled: bool,
    pub cert_file_name: Option<String>,
    pub key_file_name: Option<String>,
    pub cert_bytes: Vec<u8>,
    pub key_bytes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateEnabledInput {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateMetadataInput {
    pub name: String,
    pub hosts: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CertificateIndex {
    certificates: Vec<CertificateConfig>,
}

pub struct CertificateStore {
    data_dir: PathBuf,
    certificates: RwLock<Vec<CertificateConfig>>,
    counter: AtomicU64,
}

impl CertificateStore {
    pub async fn open(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let certificates_dir = data_dir.join("certificates");
        if let Err(err) = tokio::fs::create_dir_all(&certificates_dir).await {
            tracing::warn!("failed to create certificate data directory: {err}");
        }
        let certificates = read_json::<CertificateIndex>(&index_path(&data_dir))
            .await
            .map(|index| index.certificates)
            .unwrap_or_default();
        let max_id = certificates
            .iter()
            .filter_map(|certificate| id_suffix(&certificate.id))
            .max()
            .unwrap_or(0);
        Self {
            data_dir,
            certificates: RwLock::new(certificates),
            counter: AtomicU64::new(max_id + 1),
        }
    }

    pub async fn list(&self) -> Vec<CertificateConfig> {
        self.certificates.read().await.clone()
    }

    pub async fn get(&self, id: &str) -> AppResult<CertificateConfig> {
        self.certificates
            .read()
            .await
            .iter()
            .find(|certificate| certificate.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("certificate {id} does not exist")))
    }

    pub async fn create(&self, input: CertificateUpload) -> AppResult<CertificateConfig> {
        let name = clean_name(&input.name)?;
        let hosts = clean_hosts(input.hosts)?;
        validate_key(&input.key_bytes)?;
        validate_identity(&input.cert_bytes, &input.key_bytes)?;
        let metadata = certificate_metadata(&input.cert_bytes)?;

        let now = chrono::Utc::now().timestamp_millis();
        let id = self.next_id();
        let certificate = CertificateConfig {
            id: id.clone(),
            name,
            hosts,
            enabled: input.enabled,
            cert_path: format!("{id}/{CERT_FILE_NAME}"),
            key_path: format!("{id}/{KEY_FILE_NAME}"),
            cert_file_name: clean_file_name(input.cert_file_name.as_deref(), CERT_FILE_NAME),
            key_file_name: clean_file_name(input.key_file_name.as_deref(), KEY_FILE_NAME),
            subject_distinguished_name: metadata.subject_distinguished_name,
            issuer_distinguished_name: metadata.issuer_distinguished_name,
            serial_number: metadata.serial_number,
            valid_from: metadata.valid_from,
            valid_until: metadata.valid_until,
            fingerprint_sha256: metadata.fingerprint_sha256,
            created_at_ms: now,
            updated_at_ms: now,
        };

        let directory = self.data_dir.join("certificates").join(&id);
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(AppError::from)?;
        tokio::fs::write(directory.join(CERT_FILE_NAME), input.cert_bytes)
            .await
            .map_err(AppError::from)?;
        tokio::fs::write(directory.join(KEY_FILE_NAME), input.key_bytes)
            .await
            .map_err(AppError::from)?;

        let mut certificates = self.certificates.write().await;
        certificates.push(certificate.clone());
        persist_index(&self.data_dir, &certificates).await?;
        Ok(certificate)
    }

    pub async fn delete(&self, id: &str) -> AppResult<()> {
        {
            let mut certificates = self.certificates.write().await;
            let index = certificates
                .iter()
                .position(|certificate| certificate.id == id)
                .ok_or_else(|| AppError::NotFound(format!("certificate {id} does not exist")))?;
            certificates.remove(index);
            persist_index(&self.data_dir, &certificates).await?;
        }

        match tokio::fs::remove_dir_all(self.data_dir.join("certificates").join(id)).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(AppError::from(err)),
        }
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<CertificateConfig> {
        let mut certificates = self.certificates.write().await;
        let certificate = certificates
            .iter_mut()
            .find(|certificate| certificate.id == id)
            .ok_or_else(|| AppError::NotFound(format!("certificate {id} does not exist")))?;
        certificate.enabled = enabled;
        certificate.updated_at_ms = chrono::Utc::now().timestamp_millis();
        let saved = certificate.clone();
        persist_index(&self.data_dir, &certificates).await?;
        Ok(saved)
    }

    pub async fn update_metadata(
        &self,
        id: &str,
        input: CertificateMetadataInput,
    ) -> AppResult<CertificateConfig> {
        let name = clean_name(&input.name)?;
        let hosts = clean_hosts(input.hosts)?;
        let mut certificates = self.certificates.write().await;
        let certificate = certificates
            .iter_mut()
            .find(|certificate| certificate.id == id)
            .ok_or_else(|| AppError::NotFound(format!("certificate {id} does not exist")))?;
        certificate.name = name;
        certificate.hosts = hosts;
        certificate.enabled = input.enabled;
        certificate.updated_at_ms = chrono::Utc::now().timestamp_millis();
        let saved = certificate.clone();
        persist_index(&self.data_dir, &certificates).await?;
        Ok(saved)
    }

    pub async fn match_host(&self, host: &str) -> Option<CertificateMatch> {
        let host = normalize_host(host);
        let certificates = self.certificates.read().await;
        let mut best: Option<MatchCandidate> = None;

        for (order, certificate) in certificates.iter().enumerate() {
            if !certificate.enabled {
                continue;
            }
            for pattern in &certificate.hosts {
                let Some(specificity) = match_pattern(&host, pattern) else {
                    continue;
                };
                let candidate = MatchCandidate {
                    exact: specificity.exact,
                    suffix_len: specificity.suffix_len,
                    order,
                    certificate,
                    pattern,
                };
                if best.as_ref().is_none_or(|current| candidate.beats(current)) {
                    best = Some(candidate);
                }
            }
        }

        best.map(|candidate| CertificateMatch {
            id: candidate.certificate.id.clone(),
            name: candidate.certificate.name.clone(),
            matched_host_pattern: candidate.pattern.clone(),
        })
    }

    pub async fn identity(&self, id: &str) -> AppResult<Identity> {
        let certificate = self.get(id).await?;
        let cert_bytes = tokio::fs::read(
            self.data_dir
                .join("certificates")
                .join(&certificate.cert_path),
        )
        .await
        .map_err(AppError::from)?;
        let key_bytes = tokio::fs::read(
            self.data_dir
                .join("certificates")
                .join(&certificate.key_path),
        )
        .await
        .map_err(AppError::from)?;
        validate_key(&key_bytes)?;
        identity_from_parts(&cert_bytes, &key_bytes)
    }

    fn next_id(&self) -> String {
        let next = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("cert-{}-{next}", chrono::Utc::now().timestamp_millis())
    }
}

#[derive(Debug)]
struct MatchCandidate<'a> {
    exact: bool,
    suffix_len: usize,
    order: usize,
    certificate: &'a CertificateConfig,
    pattern: &'a String,
}

impl MatchCandidate<'_> {
    fn beats(&self, other: &Self) -> bool {
        self.exact
            .cmp(&other.exact)
            .then_with(|| self.suffix_len.cmp(&other.suffix_len))
            .then_with(|| other.order.cmp(&self.order))
            .is_gt()
    }
}

#[derive(Debug)]
struct MatchSpecificity {
    exact: bool,
    suffix_len: usize,
}

fn match_pattern(host: &str, pattern: &str) -> Option<MatchSpecificity> {
    let pattern = normalize_host(pattern);
    if host == pattern {
        return Some(MatchSpecificity {
            exact: true,
            suffix_len: pattern.len(),
        });
    }

    let suffix = pattern.strip_prefix("*.")?;
    let suffix_with_dot = format!(".{suffix}");
    let prefix = host.strip_suffix(&suffix_with_dot)?;
    if prefix.is_empty() || prefix.contains('.') {
        return None;
    }
    Some(MatchSpecificity {
        exact: false,
        suffix_len: suffix.len(),
    })
}

fn clean_hosts(hosts: Vec<String>) -> AppResult<Vec<String>> {
    let mut cleaned = Vec::new();
    for host in hosts {
        let host = normalize_host(&host);
        if host.is_empty() {
            continue;
        }
        validate_host_pattern(&host)?;
        cleaned.push(host);
    }
    if cleaned.is_empty() {
        return Err(AppError::BadRequest(
            "at least one host pattern is required".into(),
        ));
    }
    Ok(cleaned)
}

fn validate_host_pattern(pattern: &str) -> AppResult<()> {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        if suffix.contains('*') || suffix.is_empty() {
            return Err(AppError::BadRequest("invalid wildcard host pattern".into()));
        }
        validate_host_name(suffix)?;
        return Ok(());
    }
    if pattern.contains('*') {
        return Err(AppError::BadRequest(
            "wildcards must use the form *.example.com".into(),
        ));
    }
    validate_host_name(pattern)
}

fn validate_host_name(host: &str) -> AppResult<()> {
    if host.is_empty()
        || host.starts_with('.')
        || host.ends_with('.')
        || host.contains("..")
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    {
        return Err(AppError::BadRequest(format!("invalid host pattern {host}")));
    }
    Ok(())
}

fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("certificate name is required".into()));
    }
    Ok(name.to_string())
}

fn clean_file_name(value: Option<&str>, fallback: &str) -> String {
    value
        .and_then(|name| Path::new(name).file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn validate_key(bytes: &[u8]) -> AppResult<()> {
    let text = String::from_utf8_lossy(bytes).to_ascii_uppercase();
    if text.contains("ENCRYPTED") {
        return Err(AppError::BadRequest(
            "encrypted private keys are not supported yet".into(),
        ));
    }
    Ok(())
}

fn validate_identity(cert_bytes: &[u8], key_bytes: &[u8]) -> AppResult<()> {
    identity_from_parts(cert_bytes, key_bytes).map(|_| ())
}

#[derive(Debug)]
struct CertificateMetadata {
    subject_distinguished_name: String,
    issuer_distinguished_name: String,
    serial_number: String,
    valid_from: String,
    valid_until: String,
    fingerprint_sha256: String,
}

fn certificate_metadata(cert_bytes: &[u8]) -> AppResult<CertificateMetadata> {
    let (_, pem) = parse_x509_pem(cert_bytes)
        .map_err(|err| AppError::BadRequest(format!("invalid certificate PEM: {err}")))?;
    let certificate = pem
        .parse_x509()
        .map_err(|err| AppError::BadRequest(format!("invalid X.509 certificate: {err}")))?;
    Ok(CertificateMetadata {
        subject_distinguished_name: certificate.subject().to_string(),
        issuer_distinguished_name: certificate.issuer().to_string(),
        serial_number: certificate.raw_serial_as_string(),
        valid_from: certificate.validity().not_before.to_string(),
        valid_until: certificate.validity().not_after.to_string(),
        fingerprint_sha256: sha256_hex(&pem.contents),
    })
}

fn identity_from_parts(cert_bytes: &[u8], key_bytes: &[u8]) -> AppResult<Identity> {
    let mut pem = Vec::with_capacity(cert_bytes.len() + key_bytes.len() + 1);
    pem.extend_from_slice(cert_bytes);
    if !pem.ends_with(b"\n") {
        pem.push(b'\n');
    }
    pem.extend_from_slice(key_bytes);
    Identity::from_pem(&pem)
        .map_err(|err| AppError::BadRequest(format!("invalid certificate or private key: {err}")))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("certificates").join("index.json")
}

async fn persist_index(data_dir: &Path, certificates: &[CertificateConfig]) -> AppResult<()> {
    write_json(
        &index_path(data_dir),
        &CertificateIndex {
            certificates: certificates.into(),
        },
    )
    .await
}

async fn read_json<T>(path: &Path) -> AppResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(AppError::from)?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::Internal(format!("failed to parse {}: {err}", path.display())))
}

async fn write_json<T>(path: &Path, value: &T) -> AppResult<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppError::from)?;
    }
    let raw = serde_json::to_vec_pretty(value)
        .map_err(|err| AppError::Internal(format!("failed to serialize JSON: {err}")))?;
    tokio::fs::write(path, raw).await.map_err(AppError::from)
}

fn id_suffix(id: &str) -> Option<u64> {
    id.rsplit('-').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cert(id: &str, hosts: &[&str]) -> CertificateConfig {
        CertificateConfig {
            id: id.into(),
            name: id.into(),
            hosts: hosts.iter().map(|host| host.to_string()).collect(),
            enabled: true,
            cert_path: format!("{id}/client.crt"),
            key_path: format!("{id}/client.key"),
            cert_file_name: "client.crt".into(),
            key_file_name: "client.key".into(),
            subject_distinguished_name: "CN=test subject".into(),
            issuer_distinguished_name: "CN=test issuer".into(),
            serial_number: "01".into(),
            valid_from: "2026-01-01 00:00:00 UTC".into(),
            valid_until: "2027-01-01 00:00:00 UTC".into(),
            fingerprint_sha256: "fingerprint".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn wildcard_matches_one_label_only() {
        assert!(match_pattern("api.example.com", "*.example.com").is_some());
        assert!(match_pattern("example.com", "*.example.com").is_none());
        assert!(match_pattern("deep.api.example.com", "*.example.com").is_none());
    }

    #[tokio::test]
    async fn exact_match_beats_wildcard() {
        let store = CertificateStore {
            data_dir: PathBuf::new(),
            certificates: RwLock::new(vec![
                cert("wildcard", &["*.example.com"]),
                cert("exact", &["api.example.com"]),
            ]),
            counter: AtomicU64::new(1),
        };

        let matched = store.match_host("api.example.com").await.unwrap();
        assert_eq!(matched.id, "exact");
        assert_eq!(matched.matched_host_pattern, "api.example.com");
    }

    #[tokio::test]
    async fn longer_suffix_beats_shorter_suffix() {
        let store = CertificateStore {
            data_dir: PathBuf::new(),
            certificates: RwLock::new(vec![
                cert("short", &["*.example.com"]),
                cert("long", &["*.internal.example.com"]),
            ]),
            counter: AtomicU64::new(1),
        };

        let matched = store.match_host("api.internal.example.com").await.unwrap();
        assert_eq!(matched.id, "long");
    }

    #[tokio::test]
    async fn order_breaks_remaining_ties() {
        let store = CertificateStore {
            data_dir: PathBuf::new(),
            certificates: RwLock::new(vec![
                cert("first", &["*.example.com"]),
                cert("second", &["*.example.com"]),
            ]),
            counter: AtomicU64::new(1),
        };

        let matched = store.match_host("api.example.com").await.unwrap();
        assert_eq!(matched.id, "first");
    }

    #[tokio::test]
    async fn disabled_certificates_are_not_matched() {
        let data_dir = std::env::temp_dir().join(format!(
            "gate-keeper-cert-toggle-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = CertificateStore {
            data_dir,
            certificates: RwLock::new(vec![cert("disabled", &["api.example.com"])]),
            counter: AtomicU64::new(1),
        };
        store
            .set_enabled("disabled", false)
            .await
            .expect("toggle should succeed");

        assert!(store.match_host("api.example.com").await.is_none());
    }

    #[tokio::test]
    async fn metadata_update_changes_name_hosts_and_enabled_state() {
        let data_dir = std::env::temp_dir().join(format!(
            "gate-keeper-cert-metadata-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = CertificateStore {
            data_dir,
            certificates: RwLock::new(vec![cert("editable", &["api.example.com"])]),
            counter: AtomicU64::new(1),
        };

        let updated = store
            .update_metadata(
                "editable",
                CertificateMetadataInput {
                    name: "Edited".into(),
                    hosts: vec!["*.internal.example.com".into()],
                    enabled: true,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.name, "Edited");
        assert_eq!(updated.hosts, vec!["*.internal.example.com"]);
        assert!(store.match_host("api.example.com").await.is_none());
        assert_eq!(
            store
                .match_host("api.internal.example.com")
                .await
                .unwrap()
                .id,
            "editable"
        );
    }
}
