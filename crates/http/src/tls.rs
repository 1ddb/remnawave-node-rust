use std::io::BufReader;
use std::sync::Arc;

use rw_config::{ensure_crypto_provider, SniVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert, WebPkiClientVerifier};
use rustls::sign::CertifiedKey;
use rustls::{RootCertStore, ServerConfig};

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("certificate error: {0}")]
    Certificate(String),
    #[error("key error: {0}")]
    Key(String),
    #[error("rustls error: {0}")]
    Rustls(String),
}

pub fn build_server_config(
    ca_cert_pem: &str,
    node_cert_pem: &str,
    node_key_pem: &str,
    jwt_public_key: &str,
    sni_verification: bool,
) -> Result<Arc<ServerConfig>, TlsError> {
    ensure_crypto_provider();

    let ca_certs = read_certs(ca_cert_pem)?;
    let node_certs = read_certs(node_cert_pem)?;
    let key = read_key(node_key_pem)?;

    let mut roots = RootCertStore::empty();
    for cert in ca_certs {
        roots
            .add(cert)
            .map_err(|e| TlsError::Certificate(e.to_string()))?;
    }

    let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|e| TlsError::Certificate(e.to_string()))?;

    let builder = ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_client_cert_verifier(verifier);

    let config = if sni_verification {
        let signing_key = rustls::crypto::ring::sign::any_supported_type(&key)
            .map_err(|e| TlsError::Key(e.to_string()))?;
        let certified = Arc::new(CertifiedKey::new(node_certs, signing_key));
        let resolver = Arc::new(SniCertResolver {
            cert: certified,
            verifier: SniVerifier::new(ca_cert_pem, jwt_public_key),
        });
        builder.with_cert_resolver(resolver)
    } else {
        builder
            .with_single_cert(node_certs, key)
            .map_err(|e| TlsError::Rustls(e.to_string()))?
    };

    Ok(Arc::new(config))
}

fn read_certs(pem: &str) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let mut reader = BufReader::new(pem.as_bytes());
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::Certificate(e.to_string()))
}

fn read_key(pem: &str) -> Result<PrivateKeyDer<'static>, TlsError> {
    let mut reader = BufReader::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| TlsError::Key(e.to_string()))?
        .ok_or_else(|| TlsError::Key("no private key found".into()))
}

struct SniCertResolver {
    cert: Arc<CertifiedKey>,
    verifier: SniVerifier,
}

impl std::fmt::Debug for SniCertResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SniCertResolver")
    }
}

impl ResolvesServerCert for SniCertResolver {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let name = hello.server_name()?;
        if self.verifier.verify(name) {
            Some(self.cert.clone())
        } else {
            None
        }
    }
}
