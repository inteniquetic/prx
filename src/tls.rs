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
use arc_swap::ArcSwap;
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

/// The certificate set a connection is resolved against.
#[derive(Default)]
struct ResolverState {
    certs: Vec<Arc<LoadedCert>>,
    exact: HashMap<String, usize>,
    /// `suffix -> index` for `*.suffix` patterns.
    wildcard: HashMap<String, usize>,
    default_idx: usize,
}

/// Picks a certificate for a connection.
///
/// The set is held in an `ArcSwap` so a renewed certificate can replace an
/// expiring one while connections are being served: ACME renewal must not need
/// a restart.
pub struct CertResolver {
    state: ArcSwap<ResolverState>,
}

impl ResolverState {
    fn build(certs: Vec<(Arc<LoadedCert>, bool)>) -> Self {
        let mut state = Self::default();
        let mut default_idx = None;

        for (loaded, is_default) in certs {
            let idx = state.certs.len();
            for domain in &loaded.domains {
                let domain = domain.to_ascii_lowercase();
                match domain.strip_prefix("*.") {
                    Some(suffix) => {
                        state.wildcard.insert(suffix.to_string(), idx);
                    }
                    None => {
                        state.exact.insert(domain, idx);
                    }
                }
            }
            if is_default && default_idx.is_none() {
                default_idx = Some(idx);
            }
            state.certs.push(loaded);
        }

        // Without an explicit default, the first certificate answers for
        // clients that send no SNI or an unknown name; refusing the handshake
        // instead would be a worse failure mode.
        state.default_idx = default_idx.unwrap_or(0);
        state
    }

    fn resolve(&self, server_name: Option<&str>) -> Option<&Arc<LoadedCert>> {
        if self.certs.is_empty() {
            return None;
        }
        let Some(name) = server_name else {
            return self.certs.get(self.default_idx);
        };
        let name = name.trim().to_ascii_lowercase();

        if let Some(idx) = self.exact.get(&name) {
            return self.certs.get(*idx);
        }

        // `*.example.com` also covers `example.com`, and the longest suffix
        // wins because labels are stripped one at a time.
        let mut candidate = name.as_str();
        loop {
            if let Some(idx) = self.wildcard.get(candidate) {
                return self.certs.get(*idx);
            }
            match candidate.find('.') {
                Some(dot) => candidate = &candidate[dot + 1..],
                None => break,
            }
        }

        self.certs.get(self.default_idx)
    }
}

impl CertResolver {
    /// Loads and validates every configured certificate.
    pub fn load(configs: &[TlsCertConfig]) -> anyhow::Result<Self> {
        if configs.is_empty() {
            bail!("TLS is enabled but no certificate is configured");
        }

        let mut certs = Vec::with_capacity(configs.len());
        for config in configs {
            certs.push((Arc::new(load_cert(config)?), config.is_default));
        }

        Ok(Self {
            state: ArcSwap::from_pointee(ResolverState::build(certs)),
        })
    }

    /// An empty resolver, for a listener whose certificates will arrive from
    /// ACME. It serves nothing until the first certificate is installed.
    pub fn empty() -> Self {
        Self {
            state: ArcSwap::from_pointee(ResolverState::default()),
        }
    }

    /// Installs or replaces a certificate for the domains it covers, without
    /// interrupting connections in flight.
    pub fn install(
        &self,
        cert_pem: &[u8],
        key_pem: &[u8],
        domains: Vec<String>,
    ) -> anyhow::Result<()> {
        let loaded = Arc::new(load_cert_from_pem(cert_pem, key_pem, domains, "acme")?);
        let replacing: Vec<String> = loaded.domains.clone();

        let current = self.state.load();
        let mut certs: Vec<(Arc<LoadedCert>, bool)> = Vec::with_capacity(current.certs.len() + 1);
        for (idx, cert) in current.certs.iter().enumerate() {
            // Drop any certificate whose domains this one takes over.
            if cert.domains.iter().all(|domain| replacing.contains(domain)) {
                continue;
            }
            certs.push((cert.clone(), idx == current.default_idx));
        }
        let is_default = certs.is_empty();
        certs.push((loaded, is_default));

        self.state.store(Arc::new(ResolverState::build(certs)));
        self.report_expiry();
        Ok(())
    }

    /// Publishes how long each certificate has left, so expiry can be alerted
    /// on rather than discovered by users.
    pub fn report_expiry(&self) {
        let now = now_epoch_s();
        for cert in &self.state.load().certs {
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
        self.state.load().certs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.state.load().certs.is_empty()
    }

    /// Domains currently served, with the epoch second each expires at.
    pub fn domain_expiry(&self) -> Vec<(String, i64)> {
        let state = self.state.load();
        state
            .certs
            .iter()
            .flat_map(|cert| {
                cert.domains
                    .iter()
                    .map(|domain| (domain.clone(), cert.not_after_epoch_s))
            })
            .collect()
    }
}

/// Lets an `Arc<CertResolver>` be handed to pingora while the ACME task keeps
/// its own handle for installing renewed certificates.
pub struct ResolverHandle(pub Arc<CertResolver>);

#[async_trait]
impl TlsAccept for ResolverHandle {
    async fn certificate_callback(&self, ssl: &mut SslRef) {
        self.0.certificate_callback(ssl).await
    }
}

#[async_trait]
impl TlsAccept for CertResolver {
    async fn certificate_callback(&self, ssl: &mut SslRef) {
        let server_name = ssl.servername(NameType::HOST_NAME).map(str::to_string);
        let state = self.state.load();
        let Some(cert) = state.resolve(server_name.as_deref()) else {
            // No certificate yet (ACME has not issued one): let the handshake
            // fail rather than presenting something wrong.
            warn!(
                server_name = server_name.as_deref().unwrap_or("-"),
                "no TLS certificate is available yet"
            );
            return;
        };

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

/// Reads one certificate and its key from disk.
fn load_cert(config: &TlsCertConfig) -> anyhow::Result<LoadedCert> {
    let cert_bytes = std::fs::read(&config.cert_path)
        .with_context(|| format!("failed to read certificate {}", config.cert_path))?;
    let key_bytes = std::fs::read(&config.key_path)
        .with_context(|| format!("failed to read private key {}", config.key_path))?;

    load_cert_from_pem(
        &cert_bytes,
        &key_bytes,
        config.domains.clone(),
        &config.cert_path,
    )
    .with_context(|| format!("certificate {}", config.cert_path))
}

/// Parses a certificate and key, refusing anything that would fail later during
/// a handshake.
fn load_cert_from_pem(
    cert_bytes: &[u8],
    key_bytes: &[u8],
    configured_domains: Vec<String>,
    source: &str,
) -> anyhow::Result<LoadedCert> {
    let mut chain = X509::stack_from_pem(cert_bytes).context("certificate is not valid PEM")?;
    if chain.is_empty() {
        bail!("no certificate found in the PEM data");
    }
    let leaf = chain.remove(0);

    let key = PKey::private_key_from_pem(key_bytes).context("private key is not valid PEM")?;

    // A key that does not match the certificate produces a handshake failure
    // for every client, so catch it at load time instead.
    let public_key = leaf.public_key().context("no usable public key")?;
    if !key.public_eq(&public_key) {
        bail!("private key does not match certificate");
    }

    let not_after_epoch_s = asn1_time_to_epoch_s(leaf.not_after().to_string().as_str())
        .unwrap_or_else(|| now_epoch_s() + 365 * 86_400);

    let domains = if configured_domains.is_empty() {
        domains_from_cert(&leaf)
    } else {
        configured_domains
    };
    if domains.is_empty() {
        bail!("certificate lists no domains and none could be read from it");
    }

    Ok(LoadedCert {
        domains,
        leaf,
        chain,
        key,
        not_after_epoch_s,
        source: source.to_string(),
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
pub fn parse_not_after(text: &str) -> Option<i64> {
    asn1_time_to_epoch_s(text)
}

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
