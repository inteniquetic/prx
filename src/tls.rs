//! TLS certificate selection by SNI (T111).
//!
//! A proxy usually fronts more than one domain, and a single certificate does
//! not cover them. This module loads every configured certificate once at
//! startup, checks each one before it can break a handshake, and picks the
//! right one per connection from the SNI the client sent.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use async_trait::async_trait;
use pingora::listeners::TlsAccept;
use pingora::tls::{
    ext::{ssl_add_chain_cert, ssl_use_certificate, ssl_use_private_key},
    pkey::{PKey, Private},
    ssl::{NameType, SslRef},
    x509::X509,
};
use tracing::{info, warn};

use crate::{config::TlsCertConfig, metrics};

/// Warn this far ahead of a certificate expiring, so it is noticed before it
/// takes the site down.
const EXPIRY_WARN_DAYS: i64 = 14;

/// One loaded certificate and the domains it serves.
pub struct LoadedCert {
    pub domains: Vec<String>,
    pub leaf: X509,
    /// Intermediates from the same PEM file, sent after the leaf.
    pub chain: Vec<X509>,
    pub key: PKey<Private>,
    pub not_after_epoch_s: i64,
    pub source: String,
}

/// Picks a certificate for a connection.
pub struct CertResolver {
    certs: Vec<Arc<LoadedCert>>,
    exact: HashMap<String, usize>,
    /// `(suffix, index)` for `*.suffix` patterns.
    wildcard: HashMap<String, usize>,
    default_idx: usize,
}

impl CertResolver {
    /// Loads and validates every configured certificate.
    pub fn load(configs: &[TlsCertConfig]) -> anyhow::Result<Self> {
        if configs.is_empty() {
            bail!("TLS is enabled but no certificate is configured");
        }

        let mut certs = Vec::with_capacity(configs.len());
        let mut exact = HashMap::new();
        let mut wildcard = HashMap::new();
        let mut default_idx = None;

        for config in configs {
            let loaded = Arc::new(load_cert(config)?);
            let idx = certs.len();

            for domain in &loaded.domains {
                let domain = domain.to_ascii_lowercase();
                match domain.strip_prefix("*.") {
                    Some(suffix) => {
                        wildcard.insert(suffix.to_string(), idx);
                    }
                    None => {
                        exact.insert(domain, idx);
                    }
                }
            }
            if config.is_default && default_idx.is_none() {
                default_idx = Some(idx);
            }
            certs.push(loaded);
        }

        Ok(Self {
            // Without an explicit default, the first certificate answers for
            // clients that send no SNI or an unknown name; refusing the
            // handshake instead would be a worse failure mode.
            default_idx: default_idx.unwrap_or(0),
            certs,
            exact,
            wildcard,
        })
    }

    /// Resolves a server name to a certificate.
    pub fn resolve(&self, server_name: Option<&str>) -> &Arc<LoadedCert> {
        let Some(name) = server_name else {
            return &self.certs[self.default_idx];
        };
        let name = name.trim().to_ascii_lowercase();

        if let Some(idx) = self.exact.get(&name) {
            return &self.certs[*idx];
        }

        // `*.example.com` also covers `example.com`, and the longest suffix
        // wins because labels are stripped one at a time.
        let mut candidate = name.as_str();
        loop {
            if let Some(idx) = self.wildcard.get(candidate) {
                return &self.certs[*idx];
            }
            match candidate.find('.') {
                Some(dot) => candidate = &candidate[dot + 1..],
                None => break,
            }
        }

        &self.certs[self.default_idx]
    }

    /// Publishes how long each certificate has left, so expiry can be alerted
    /// on rather than discovered by users.
    pub fn report_expiry(&self) {
        let now = now_epoch_s();
        for cert in &self.certs {
            let remaining = cert.not_after_epoch_s - now;
            for domain in &cert.domains {
                metrics::set_tls_cert_expiry(domain, remaining);
            }
            let days = remaining / 86_400;
            if days <= EXPIRY_WARN_DAYS {
                warn!(
                    source = cert.source.as_str(),
                    domains = cert.domains.join(",").as_str(),
                    days_remaining = days,
                    "TLS certificate expires soon"
                );
            } else {
                info!(
                    source = cert.source.as_str(),
                    domains = cert.domains.join(",").as_str(),
                    days_remaining = days,
                    "loaded TLS certificate"
                );
            }
        }
    }

    pub fn len(&self) -> usize {
        self.certs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.certs.is_empty()
    }
}

#[async_trait]
impl TlsAccept for CertResolver {
    async fn certificate_callback(&self, ssl: &mut SslRef) {
        let server_name = ssl.servername(NameType::HOST_NAME).map(str::to_string);
        let cert = self.resolve(server_name.as_deref());

        if let Err(err) = ssl_use_certificate(ssl, &cert.leaf) {
            warn!(error = %err, "failed to install TLS certificate");
            return;
        }
        if let Err(err) = ssl_use_private_key(ssl, &cert.key) {
            warn!(error = %err, "failed to install TLS private key");
            return;
        }
        for intermediate in &cert.chain {
            if let Err(err) = ssl_add_chain_cert(ssl, intermediate) {
                warn!(error = %err, "failed to add intermediate certificate");
            }
        }
    }
}

/// Reads one certificate and its key, and refuses anything that would fail
/// later during a handshake.
fn load_cert(config: &TlsCertConfig) -> anyhow::Result<LoadedCert> {
    let cert_bytes = std::fs::read(&config.cert_path)
        .with_context(|| format!("failed to read certificate {}", config.cert_path))?;
    let key_bytes = std::fs::read(&config.key_path)
        .with_context(|| format!("failed to read private key {}", config.key_path))?;

    let mut chain = X509::stack_from_pem(&cert_bytes)
        .with_context(|| format!("{} is not a valid PEM certificate", config.cert_path))?;
    if chain.is_empty() {
        bail!("{} contains no certificate", config.cert_path);
    }
    let leaf = chain.remove(0);

    let key = PKey::private_key_from_pem(&key_bytes)
        .with_context(|| format!("{} is not a valid PEM private key", config.key_path))?;

    // A key that does not match the certificate produces a handshake failure
    // for every client, so catch it at load time instead.
    let public_key = leaf
        .public_key()
        .with_context(|| format!("{} has no usable public key", config.cert_path))?;
    if !key.public_eq(&public_key) {
        bail!(
            "private key {} does not match certificate {}",
            config.key_path,
            config.cert_path
        );
    }

    let not_after_epoch_s = asn1_time_to_epoch_s(leaf.not_after().to_string().as_str())
        .unwrap_or_else(|| now_epoch_s() + 365 * 86_400);

    let domains = if config.domains.is_empty() {
        domains_from_cert(&leaf)
    } else {
        config.domains.clone()
    };
    if domains.is_empty() {
        bail!(
            "certificate {} lists no domains and none could be read from it",
            config.cert_path
        );
    }

    Ok(LoadedCert {
        domains,
        leaf,
        chain,
        key,
        not_after_epoch_s,
        source: config.cert_path.clone(),
    })
}

/// Reads the names a certificate is valid for, so a config does not have to
/// repeat what the certificate already says.
fn domains_from_cert(cert: &X509) -> Vec<String> {
    let mut domains = Vec::new();

    if let Some(alt_names) = cert.subject_alt_names() {
        for name in alt_names.iter() {
            if let Some(dns) = name.dnsname() {
                domains.push(dns.to_ascii_lowercase());
            }
        }
    }
    if domains.is_empty() {
        for entry in cert.subject_name().entries() {
            // to_string rather than as_utf8: the latter is deprecated because
            // it truncates at an interior NUL byte.
            if let Ok(value) = entry.data().to_string() {
                domains.push(value.to_ascii_lowercase());
            }
        }
    }
    domains
}

/// Parses OpenSSL's `not_after` rendering (`Mon DD HH:MM:SS YYYY GMT`).
fn asn1_time_to_epoch_s(text: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    let mut parts = text.split_whitespace();
    let month = parts.next()?;
    let day: i64 = parts.next()?.parse().ok()?;
    let time = parts.next()?;
    let year: i64 = parts.next()?.parse().ok()?;

    let month_idx = MONTHS.iter().position(|m| *m == month)? as i64 + 1;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next()?.parse().ok()?;

    Some(days_from_civil(year, month_idx, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days since the Unix epoch for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn now_epoch_s() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openssl_not_after_format() {
        // 2026-09-18T01:02:03Z
        let parsed = asn1_time_to_epoch_s("Sep 18 01:02:03 2026 GMT").expect("parsed");
        assert_eq!(parsed, 1_789_693_323);

        assert_eq!(asn1_time_to_epoch_s("nonsense"), None);
    }

    #[test]
    fn civil_dates_match_known_epochs() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2026, 1, 1), 20_454);
    }
}
