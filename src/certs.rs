//! Digital certificates: client certificates (mutual TLS) and private certificate authorities.
//!
//! Each entry is tied to a host. When a request goes to a matching host, the client
//! certificate is presented to the server, and the entry's CA certificate (if any) is
//! trusted in addition to the usual public authorities.

use std::fmt;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CertKind {
    /// A PKCS#12 bundle (`.pfx` / `.p12`) holding the certificate and its private key.
    #[default]
    Pfx,
    /// A certificate file and a separate private key file, both PEM.
    Pem,
}

impl CertKind {
    pub const ALL: [CertKind; 2] = [CertKind::Pfx, CertKind::Pem];
}

impl fmt::Display for CertKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CertKind::Pfx => "PFX / PKCS#12",
            CertKind::Pem => "PEM (CRT + KEY)",
        })
    }
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCert {
    #[serde(default = "yes")]
    pub enabled: bool,
    /// `api.example.com`, `*.example.com` or `host:port`.
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub kind: CertKind,
    #[serde(default)]
    pub pfx_path: String,
    #[serde(default)]
    pub cert_path: String,
    #[serde(default)]
    pub key_path: String,
    #[serde(default)]
    pub passphrase: String,
    /// Optional extra authority to trust for this host (PEM or DER).
    #[serde(default)]
    pub ca_path: String,
}

impl Default for ClientCert {
    fn default() -> Self {
        Self {
            enabled: true,
            host: String::new(),
            kind: CertKind::Pfx,
            pfx_path: String::new(),
            cert_path: String::new(),
            key_path: String::new(),
            passphrase: String::new(),
            ca_path: String::new(),
        }
    }
}

impl ClientCert {
    /// Whether this entry applies to a request for `host` (and `port`).
    pub fn matches(&self, host: &str, port: Option<u16>) -> bool {
        let pattern = self.host.trim().to_lowercase();
        let pattern = pattern
            .split_once("://")
            .map_or(pattern.as_str(), |(_, rest)| rest);
        let pattern = pattern.split('/').next().unwrap_or("");
        if pattern.is_empty() {
            return false;
        }
        let (name, wanted_port) = match pattern.rsplit_once(':') {
            Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
                (name, port.parse::<u16>().ok())
            }
            _ => (pattern, None),
        };
        if let (Some(wanted), Some(actual)) = (wanted_port, port) {
            if wanted != actual {
                return false;
            }
        }

        let host = host.to_lowercase();
        if name == "*" {
            true
        } else if let Some(suffix) = name.strip_prefix("*.") {
            host.len() > suffix.len() + 1 && host.ends_with(&format!(".{suffix}"))
        } else {
            host == name
        }
    }

    fn has_identity(&self) -> bool {
        match self.kind {
            CertKind::Pfx => !self.pfx_path.trim().is_empty(),
            CertKind::Pem => {
                !self.cert_path.trim().is_empty() || !self.key_path.trim().is_empty()
            }
        }
    }

    /// The client certificate and key, ready for the HTTP client.
    pub fn identity(&self) -> Result<Option<reqwest::Identity>, String> {
        if !self.has_identity() {
            return Ok(None);
        }
        let pem = match self.kind {
            CertKind::Pfx => {
                let path = self.pfx_path.trim();
                let bytes = std::fs::read(path)
                    .map_err(|e| format!("Could not read the PFX file: {e}"))?;
                pfx_to_pem(&bytes, &self.passphrase)?
            }
            CertKind::Pem => {
                if self.cert_path.trim().is_empty() {
                    return Err("Choose the certificate file.".into());
                }
                if self.key_path.trim().is_empty() {
                    return Err("Choose the private key file.".into());
                }
                let mut pem = std::fs::read(self.key_path.trim())
                    .map_err(|e| format!("Could not read the private key: {e}"))?;
                pem.push(b'\n');
                pem.extend(
                    std::fs::read(self.cert_path.trim())
                        .map_err(|e| format!("Could not read the certificate: {e}"))?,
                );
                pem
            }
        };
        reqwest::Identity::from_pem(&pem)
            .map(Some)
            .map_err(|e| format!("The certificate or key is not valid: {e}"))
    }

    /// The extra certificate authorities to trust.
    pub fn authorities(&self) -> Result<Vec<reqwest::Certificate>, String> {
        let path = self.ca_path.trim();
        if path.is_empty() {
            return Ok(Vec::new());
        }
        let bytes =
            std::fs::read(path).map_err(|e| format!("Could not read the CA certificate: {e}"))?;
        if bytes.windows(11).any(|w| w == b"-----BEGIN ") {
            let list = reqwest::Certificate::from_pem_bundle(&bytes)
                .map_err(|e| format!("The CA certificate is not valid: {e}"))?;
            if list.is_empty() {
                return Err("The CA file has no certificate in it.".into());
            }
            Ok(list)
        } else {
            reqwest::Certificate::from_der(&bytes)
                .map(|c| vec![c])
                .map_err(|e| format!("The CA certificate is not valid: {e}"))
        }
    }

    /// Checks that the files can be read and understood.
    pub fn check(&self) -> Result<(), String> {
        if self.host.trim().is_empty() {
            return Err("Enter the host this certificate is for.".into());
        }
        if !self.has_identity() && self.ca_path.trim().is_empty() {
            return Err("Choose a certificate file.".into());
        }
        self.identity()?;
        self.authorities()?;
        Ok(())
    }
}

/// Converts a PKCS#12 bundle to the PEM text the TLS library reads (key first, then the
/// certificate chain, own certificate first).
fn pfx_to_pem(bytes: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    let store = p12_keystore::KeyStore::from_pkcs12(
        bytes,
        passphrase,
        p12_keystore::Pkcs12ImportPolicy::Relaxed,
    ).map_err(|e| {
        format!("Could not open the PFX file (wrong passphrase or unsupported format): {e}")
    })?;
    let (_, chain) = store
        .private_key_chain()
        .ok_or("The PFX file has no private key.")?;

    let mut out = String::new();
    out.push_str(&pem::encode(&pem::Pem::new(
        "PRIVATE KEY",
        chain.key().as_der().to_vec(),
    )));
    for certificate in chain.certs() {
        out.push_str(&pem::encode(&pem::Pem::new(
            "CERTIFICATE",
            certificate.as_der().to_vec(),
        )));
    }
    Ok(out.into_bytes())
}

// ───────────────────────────── in use by the HTTP client ─────────────────────────────

static ACTIVE: RwLock<Vec<ClientCert>> = RwLock::new(Vec::new());

/// Makes `list` the certificates used for the next requests.
pub fn set_active(list: &[ClientCert]) {
    if let Ok(mut active) = ACTIVE.write() {
        *active = list.to_vec();
    }
}

/// Adds the certificate that belongs to `url` (if any) to an HTTP client under construction.
pub fn apply(
    mut builder: reqwest::ClientBuilder,
    url: &reqwest::Url,
) -> Result<reqwest::ClientBuilder, String> {
    let Some(host) = url.host_str() else {
        return Ok(builder);
    };
    let entry = ACTIVE.read().ok().and_then(|list| {
        list.iter()
            .find(|c| c.enabled && c.matches(host, url.port_or_known_default()))
            .cloned()
    });
    let Some(entry) = entry else {
        return Ok(builder);
    };

    if let Some(identity) = entry.identity().map_err(|e| format!("Certificate for {}: {e}", entry.host.trim()))? {
        builder = builder.identity(identity);
    }
    for authority in entry
        .authorities()
        .map_err(|e| format!("Certificate for {}: {e}", entry.host.trim()))?
    {
        builder = builder.add_root_certificate(authority);
    }
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(host: &str) -> ClientCert {
        ClientCert {
            host: host.into(),
            ..ClientCert::default()
        }
    }

    #[test]
    fn matches_hosts() {
        assert!(entry("api.example.com").matches("API.example.com", Some(443)));
        assert!(!entry("api.example.com").matches("example.com", Some(443)));
        assert!(entry("*.example.com").matches("a.b.example.com", None));
        assert!(!entry("*.example.com").matches("example.com", None));
        assert!(entry("https://api.example.com/v1").matches("api.example.com", Some(443)));
        assert!(entry("api.example.com:8443").matches("api.example.com", Some(8443)));
        assert!(!entry("api.example.com:8443").matches("api.example.com", Some(443)));
        assert!(entry("*").matches("anything", None));
        assert!(!entry("").matches("anything", None));
    }

    #[test]
    fn explains_missing_files() {
        assert!(entry("").check().is_err());
        let mut cert = entry("x.com");
        assert!(cert.check().is_err()); // nothing chosen
        cert.pfx_path = "/definitely/not/here.pfx".into();
        assert!(cert.check().unwrap_err().contains("Could not read"));
    }
}
