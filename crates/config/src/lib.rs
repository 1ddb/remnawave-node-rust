use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use hkdf::Hkdf;
use rand::Rng;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;
use x509_parser::prelude::*;

pub const HKDF_INFO: &[u8] = b"rw-v1";
pub const TLDS: [&str; 6] = ["com", "net", "org", "io", "dev", "app"];

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("SECRET_KEY missing in environment variables.")]
    MissingSecretKey,
    #[error("SECRET_KEY contains invalid base64 or JSON")]
    InvalidEncoding,
    #[error("Invalid SECRET_KEY payload structure")]
    InvalidStructure,
    #[error("SECRET_KEY payload validation failed: {0}")]
    InvalidPayload(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePayload {
    #[serde(rename = "caCertPem")]
    pub ca_cert_pem: String,
    #[serde(rename = "jwtPublicKey")]
    pub jwt_public_key: String,
    #[serde(rename = "nodeCertPem")]
    pub node_cert_pem: String,
    #[serde(rename = "nodeKeyPem")]
    pub node_key_pem: String,
}

#[derive(Debug, Clone)]
pub struct PayloadCheck {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

pub fn normalize_pem(pem: &str) -> String {
    let pem = pem.replace("\\n", "\n");
    let pem = pem.replace("\r\n", "\n");
    // Re-insert a newline after BEGIN and before END markers, then collapse.
    let mut out = String::with_capacity(pem.len());
    for line in pem.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("-----BEGIN") {
            out.push_str(trimmed);
            out.push('\n');
        } else if trimmed.starts_with("-----END") {
            out.push_str(trimmed);
            out.push('\n');
        } else if !trimmed.is_empty() {
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// Decode a base64 `SECRET_KEY` into its PEM payload.
pub fn parse_node_payload(secret_key: &str) -> Result<NodePayload, PayloadError> {
    // Node's `Buffer.from(value, "base64")` silently ignores characters outside
    // the base64 alphabet (quotes, whitespace, etc.), so strip them for parity.
    let sanitized: String = secret_key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '='))
        .collect();
    let raw = BASE64
        .decode(&sanitized)
        .or_else(|_| {
            base64::engine::general_purpose::STANDARD_NO_PAD.decode(&sanitized)
        })
        .map_err(|_| PayloadError::InvalidEncoding)?;
    let parsed: NodePayload =
        serde_json::from_slice(&raw).map_err(|_| PayloadError::InvalidStructure)?;
    Ok(NodePayload {
        ca_cert_pem: normalize_pem(&parsed.ca_cert_pem),
        jwt_public_key: normalize_pem(&parsed.jwt_public_key),
        node_cert_pem: normalize_pem(&parsed.node_cert_pem),
        node_key_pem: normalize_pem(&parsed.node_key_pem),
    })
}

fn canon(pem: &str) -> String {
    let mut out = String::with_capacity(pem.len());
    let bytes = pem.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"-----") {
            if let Some(rel) = pem[i + 5..].find("-----") {
                i += 5 + rel + 5;
                continue;
            }
        }
        let ch = bytes[i] as char;
        if ch.is_ascii_alphanumeric() || ch == '+' || ch == '/' || ch == '=' {
            out.push(ch);
        }
        i += 1;
    }
    out
}

/// Deterministic SNI name derived from the JWT public key and CA certificate.
pub fn derive_sni(ca_cert_pem: &str, jwt_public_key: &str) -> String {
    let mut ikm = Vec::new();
    ikm.extend_from_slice(canon(jwt_public_key).as_bytes());
    ikm.extend_from_slice(canon(ca_cert_pem).as_bytes());

    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut okm = [0u8; 22];
    hk.expand(HKDF_INFO, &mut okm)
        .expect("22 bytes is a valid HKDF output length");

    let host = hex(&okm[0..16]);
    let mid = hex(&okm[16..21]);
    let tld = TLDS[(okm[21] as usize) % TLDS.len()];
    format!("{host}.{mid}.{tld}")
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub struct SniVerifier {
    expected: Vec<u8>,
}

impl SniVerifier {
    pub fn new(ca_cert_pem: &str, jwt_public_key: &str) -> Self {
        Self {
            expected: derive_sni(ca_cert_pem, jwt_public_key).into_bytes(),
        }
    }

    pub fn verify(&self, servername: &str) -> bool {
        if servername.is_empty() {
            return false;
        }
        constant_time_eq(servername.as_bytes(), &self.expected)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn parse_cert(pem: &str) -> Result<CertificateDer<'static>, String> {
    let mut cursor = std::io::Cursor::new(pem.as_bytes());
    let certs = rustls_pemfile::certs(&mut cursor).collect::<Result<Vec<_>, _>>();
    match certs {
        Ok(certs) => certs
            .into_iter()
            .next()
            .ok_or_else(|| "no certificate found".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn parse_key(pem: &str) -> Result<PrivateKeyDer<'static>, String> {
    let mut cursor = std::io::Cursor::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut cursor)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no private key found".to_string())
}

/// Run the same integrity checks as the Node implementation. Returns all
/// results; the caller decides whether to abort.
pub fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn run_payload_checks(payload: &NodePayload) -> Vec<PayloadCheck> {
    ensure_crypto_provider();
    let mut checks = Vec::new();
    let mut add = |name: &'static str, fn_: &dyn Fn() -> Result<String, String>| {
        match fn_() {
            Ok(detail) => checks.push(PayloadCheck {
                name,
                ok: true,
                detail,
            }),
            Err(detail) => checks.push(PayloadCheck {
                name,
                ok: false,
                detail,
            }),
        }
    };

    add("CA parses", &|| {
        let der = parse_cert(&payload.ca_cert_pem)?;
        let (_, _cert) = parse_x509_certificate(der.as_ref()).map_err(|e| e.to_string())?;
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(der.as_ref());
        Ok(digest.iter().take(12).map(|b| format!("{b:02x}")).collect())
    });

    add("CA not expired", &|| {
        let der = parse_cert(&payload.ca_cert_pem)?;
        let (_, cert) = parse_x509_certificate(der.as_ref()).map_err(|e| e.to_string())?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let validity = cert.validity();
        if validity.not_before.timestamp() > now {
            return Err("not yet valid".into());
        }
        if validity.not_after.timestamp() < now {
            return Err("expired".into());
        }
        Ok(format!("until {}", validity.not_after))
    });

    add("CA self-signature", &|| {
        let der = parse_cert(&payload.ca_cert_pem)?;
        let (_, cert) = parse_x509_certificate(der.as_ref()).map_err(|e| e.to_string())?;
        if !cert.is_ca() {
            return Err("not a CA certificate".into());
        }
        cert.verify_signature(None).map_err(|e| e.to_string())?;
        Ok("valid".into())
    });

    add("node cert parses", &|| {
        let der = parse_cert(&payload.node_cert_pem)?;
        let (_, _cert) = parse_x509_certificate(der.as_ref()).map_err(|e| e.to_string())?;
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(der.as_ref());
        Ok(digest.iter().take(12).map(|b| format!("{b:02x}")).collect())
    });

    add("node signed by CA", &|| {
        let ca_der = parse_cert(&payload.ca_cert_pem)?;
        let (_, ca) = parse_x509_certificate(ca_der.as_ref()).map_err(|e| e.to_string())?;
        let node_der = parse_cert(&payload.node_cert_pem)?;
        let (_, node) = parse_x509_certificate(node_der.as_ref()).map_err(|e| e.to_string())?;
        node.verify_signature(Some(ca.public_key()))
            .map_err(|e| e.to_string())?;
        Ok("valid".into())
    });

    add("node key matches cert", &|| {
        let node_der = parse_cert(&payload.node_cert_pem)?;
        let key = parse_key(&payload.node_key_pem)?;
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![node_der], key)
            .map_err(|e| e.to_string())?;
        Ok("valid".into())
    });

    add("jwt public key", &|| {
        let mut cursor = std::io::Cursor::new(payload.jwt_public_key.as_bytes());
        let keys = rustls_pemfile::public_keys(&mut cursor).collect::<Result<Vec<_>, _>>();
        match keys {
            Ok(keys) if !keys.is_empty() => Ok("ok".into()),
            _ => Err("not a valid public key".into()),
        }
    });

    checks
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub node_port: u16,
    pub secret_key: String,
    pub jwt_public_key: String,
    pub disable_hashed_set_check: bool,
    pub internal_rest_token: String,
    pub internal_socket_path: String,
    pub xtls_api_socket_path: String,
    pub nftables_logging: bool,
    pub nftables_accept_reply_traffic: bool,
    pub sni_verification: bool,
    pub payload: NodePayload,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingVar(&'static str),
    #[error("invalid value for {0}: {1}")]
    InvalidVar(&'static str, String),
    #[error(transparent)]
    Payload(#[from] PayloadError),
}

fn bool_var(name: &'static str, default: bool) -> Result<bool, ConfigError> {
    match std::env::var(name) {
        Ok(v) if v.is_empty() => Ok(default),
        Ok(v) if v == "true" => Ok(true),
        Ok(v) if v == "false" => Ok(false),
        Ok(v) => Err(ConfigError::InvalidVar(name, v)),
        Err(_) => Ok(default),
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    std::env::var(name).map_err(|_| ConfigError::MissingVar(name))
}

fn random_alnum(len: usize) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}

/// Reuses an existing value (from the environment or `/run/rw-node.env`) and
/// generates a new one only when neither is present.
fn ensure_runtime_var(name: &str, generate: impl FnOnce() -> String) {
    if std::env::var(name).is_ok_and(|value| !value.is_empty()) {
        return;
    }
    if let Some(value) = resolve_runtime_var(name) {
        std::env::set_var(name, value);
        return;
    }
    std::env::set_var(name, generate());
}

/// Emulates the old s6 `init-env` stage: generate ephemeral runtime secrets if
/// they are not provided by the caller. Values persisted by a previous run are
/// reused so the CLI and a restarting instance keep talking to the same sockets.
pub fn init_runtime_env() {
    ensure_runtime_var("INTERNAL_REST_TOKEN", || random_alnum(64));
    ensure_runtime_var("INTERNAL_SOCKET_PATH", || {
        format!("rwint-{}", random_alnum(10))
    });
    ensure_runtime_var("XTLS_API_SOCKET_PATH", || {
        format!("xtls-api-{}", random_alnum(10))
    });
    persist_runtime_env();
}

pub fn load() -> Result<AppConfig, ConfigError> {
    let node_port: u16 = required("NODE_PORT")?
        .parse()
        .map_err(|e: std::num::ParseIntError| ConfigError::InvalidVar("NODE_PORT", e.to_string()))?;
    let secret_key = required("SECRET_KEY")?;
    let payload = parse_node_payload(&secret_key)?;
    let jwt_public_key = payload.jwt_public_key.clone();

    Ok(AppConfig {
        node_port,
        secret_key,
        jwt_public_key,
        disable_hashed_set_check: bool_var("DISABLE_HASHED_SET_CHECK", false)?,
        internal_rest_token: required("INTERNAL_REST_TOKEN")?,
        internal_socket_path: required("INTERNAL_SOCKET_PATH")?,
        xtls_api_socket_path: required("XTLS_API_SOCKET_PATH")?,
        nftables_logging: bool_var("NFTABLES_LOGGING", true)?,
        nftables_accept_reply_traffic: bool_var("NFTABLES_ACCEPT_REPLY_TRAFFIC", false)?,
        sni_verification: bool_var("SNI_VERIFICATION", false)?,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_normalization() {
        let raw = "-----BEGIN CERTIFICATE-----\\nABC\\n-----END CERTIFICATE-----";
        assert_eq!(
            normalize_pem(raw),
            "-----BEGIN CERTIFICATE-----\nABC\n-----END CERTIFICATE-----"
        );
    }

    #[test]
    fn secret_key_accepts_surrounding_quotes() {
        let json = r#"{"caCertPem":"A","jwtPublicKey":"B","nodeCertPem":"C","nodeKeyPem":"D"}"#;
        let encoded = BASE64.encode(json);
        let payload = parse_node_payload(&format!("\"{encoded}\"")).unwrap();
        assert_eq!(payload.ca_cert_pem, "A");
        assert_eq!(payload.jwt_public_key, "B");
        assert_eq!(payload.node_cert_pem, "C");
        assert_eq!(payload.node_key_pem, "D");
    }

    #[test]
    fn sni_is_stable() {
        let a = derive_sni("CA", "JWT");
        let b = derive_sni("CA", "JWT");
        assert_eq!(a, b);
        assert_eq!(a.rsplit('.').next().unwrap().len() <= 3, true);
    }
}

// ---------------------------------------------------------------------------
// Runtime environment persistence (replaces s6 container_environment)
// ---------------------------------------------------------------------------

pub const RUNTIME_ENV_FILE: &str = "/run/rw-node.env";

pub fn persist_runtime_env() {
    let vars = ["INTERNAL_REST_TOKEN", "INTERNAL_SOCKET_PATH", "XTLS_API_SOCKET_PATH"];
    let mut out = String::new();
    for name in vars {
        if let Ok(value) = std::env::var(name) {
            out.push_str(&format!("{name}={value}\n"));
        }
    }
    if let Some(parent) = std::path::Path::new(RUNTIME_ENV_FILE).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Write through a temporary file so a crash cannot leave a partially
    // written token file behind. The file is readable only by the owner:
    // it contains INTERNAL_REST_TOKEN.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let tmp_path = format!("{RUNTIME_ENV_FILE}.tmp");

        let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp_path)
        else {
            return;
        };
        if file.write_all(out.as_bytes()).is_err() || file.sync_all().is_err() {
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }
        drop(file);
        let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
        if std::fs::rename(&tmp_path, RUNTIME_ENV_FILE).is_err() {
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }
        let _ = std::fs::set_permissions(
            RUNTIME_ENV_FILE,
            std::fs::Permissions::from_mode(0o600),
        );
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::write(RUNTIME_ENV_FILE, out);
    }
}

pub fn resolve_runtime_var(name: &str) -> Option<String> {
    if let Ok(value) = std::env::var(name) {
        if !value.is_empty() {
            return Some(value);
        }
    }
    let content = std::fs::read_to_string(RUNTIME_ENV_FILE).ok()?;
    content.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        if key == name {
            Some(value.to_string())
        } else {
            None
        }
    })
}
