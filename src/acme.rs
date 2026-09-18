//! Automatic certificates over ACME (T112).
//!
//! What makes this worth having is that nobody has to touch a certificate
//! again: prx orders one for the configured domains, answers the HTTP-01
//! challenge on its own plaintext listener, installs the result into the
//! running TLS resolver without a restart, and renews before expiry.
//!
//! Failure is survivable by design. If ACME is unreachable, the certificate
//! already in use keeps serving traffic and the failure is reported through
//! `GET /web/tls/status` rather than taking the proxy down.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus, RetryPolicy,
};
use serde::Serialize;
use tracing::{error, info, warn};

use crate::{config::AcmeConfig, tls::CertResolver};

/// Where the HTTP-01 challenge is served from.
pub const CHALLENGE_PREFIX: &str = "/.well-known/acme-challenge/";

/// How often the renewal loop wakes up to check whether anything is due.
const CHECK_INTERVAL: Duration = Duration::from_secs(12 * 3_600);

/// First retry delay after a failed order; doubles up to `MAX_RETRY`.
const MIN_RETRY: Duration = Duration::from_secs(60);
const MAX_RETRY: Duration = Duration::from_secs(3_600);

/// Answers to in-flight HTTP-01 challenges, shared with the request path.
///
/// Reads vastly outnumber writes (one write per challenge, a read per probe),
/// and the map is tiny, so a plain `RwLock<HashMap>` is the right shape.
#[derive(Debug, Default)]
pub struct ChallengeStore {
    tokens: RwLock<HashMap<String, String>>,
    /// Set once at startup when ACME is enabled. It decides ownership of the
    /// challenge prefix, which is not the same question as whether a challenge
    /// happens to be in flight: with ACME on, prx answers the prefix itself so
    /// a late or forged validation never reaches an upstream; with ACME off,
    /// the prefix is an ordinary path and routes normally.
    active: AtomicBool,
}

impl ChallengeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, token: String, key_authorization: String) {
        if let Ok(mut tokens) = self.tokens.write() {
            tokens.insert(token, key_authorization);
        }
    }

    pub fn remove(&self, token: &str) {
        if let Ok(mut tokens) = self.tokens.write() {
            tokens.remove(token);
        }
    }

    pub fn get(&self, token: &str) -> Option<String> {
        self.tokens.read().ok()?.get(token).cloned()
    }

    /// Marks ACME as enabled, so prx owns `/.well-known/acme-challenge/`.
    pub fn activate(&self) {
        self.active.store(true, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
}

/// What the admin API reports about certificate automation.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AcmeStatus {
    pub enabled: bool,
    pub directory_url: String,
    pub domains: Vec<String>,
    pub staging: bool,
    /// Epoch seconds of the last attempt, successful or not.
    pub last_attempt_epoch_s: Option<i64>,
    pub last_success_epoch_s: Option<i64>,
    /// Why the last attempt failed, when it did.
    pub last_error: Option<String>,
    pub certificate_expiry_epoch_s: Option<i64>,
}

pub type SharedStatus = Arc<RwLock<AcmeStatus>>;

/// Runs the ordering and renewal loop on its own thread and runtime, so a slow
/// ACME server never competes with request handling.
pub fn spawn_acme(
    config: AcmeConfig,
    resolver: Arc<CertResolver>,
    challenges: Arc<ChallengeStore>,
    status: SharedStatus,
) -> anyhow::Result<()> {
    let storage = PathBuf::from(&config.storage_dir);
    std::fs::create_dir_all(&storage).with_context(|| {
        format!(
            "failed to create the ACME storage directory {}",
            config.storage_dir
        )
    })?;
    restrict_permissions(&storage)?;

    std::thread::Builder::new()
        .name("prx-acme".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    error!(error = %err, "failed to start the ACME runtime");
                    return;
                }
            };
            runtime.block_on(run(config, storage, resolver, challenges, status));
        })?;
    Ok(())
}

async fn run(
    config: AcmeConfig,
    storage: PathBuf,
    resolver: Arc<CertResolver>,
    challenges: Arc<ChallengeStore>,
    status: SharedStatus,
) {
    let mut backoff = MIN_RETRY;

    loop {
        let due = match certificate_due(&storage, &config) {
            Ok(due) => due,
            Err(err) => {
                warn!(error = %err, "could not inspect the stored certificate; ordering a new one");
                true
            }
        };

        if due {
            record_attempt(&status);
            match obtain(&config, &storage, &resolver, &challenges).await {
                Ok(expiry) => {
                    backoff = MIN_RETRY;
                    record_success(&status, expiry);
                    info!(
                        domains = config.domains.join(",").as_str(),
                        "ACME certificate installed"
                    );
                }
                Err(err) => {
                    // The certificate already in use keeps serving; this is
                    // reported, not fatal.
                    error!(error = %format!("{err:#}"), "ACME order failed; keeping the current certificate");
                    record_failure(&status, format!("{err:#}"));
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_RETRY);
                    continue;
                }
            }
        }

        tokio::time::sleep(CHECK_INTERVAL).await;
    }
}

/// Runs one full order: account, authorizations, challenges, finalize, install.
async fn obtain(
    config: &AcmeConfig,
    storage: &Path,
    resolver: &Arc<CertResolver>,
    challenges: &Arc<ChallengeStore>,
) -> anyhow::Result<i64> {
    let account = load_or_create_account(config, storage).await?;

    let identifiers: Vec<Identifier> = config
        .domains
        .iter()
        .map(|domain| Identifier::Dns(domain.clone()))
        .collect();
    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .context("failed to create the ACME order")?;

    // Publish every challenge answer before telling the server we are ready.
    let mut published: Vec<String> = Vec::new();
    let mut authorizations = order.authorizations();
    while let Some(result) = authorizations.next().await {
        let mut authz = result.context("failed to read an authorization")?;
        match authz.status {
            AuthorizationStatus::Pending => {}
            AuthorizationStatus::Valid => continue,
            other => bail!("authorization is in an unusable state: {other:?}"),
        }

        let mut challenge = authz
            .challenge(ChallengeType::Http01)
            .context("the ACME server offered no http-01 challenge")?;
        let token = challenge.token.to_string();
        challenges.insert(
            token.clone(),
            challenge.key_authorization().as_str().to_string(),
        );
        published.push(token);

        challenge
            .set_ready()
            .await
            .context("failed to tell the ACME server the challenge is ready")?;
    }

    let outcome = finish_order(&mut order, config, storage, resolver).await;

    // Challenge answers are only valid for this order.
    for token in published {
        challenges.remove(&token);
    }
    outcome
}

async fn finish_order(
    order: &mut instant_acme::Order,
    config: &AcmeConfig,
    storage: &Path,
    resolver: &Arc<CertResolver>,
) -> anyhow::Result<i64> {
    let status = order
        .poll_ready(&RetryPolicy::default())
        .await
        .context("the ACME order never became ready")?;
    if status != OrderStatus::Ready {
        bail!("unexpected ACME order status: {status:?}");
    }

    let key_pem = order
        .finalize()
        .await
        .context("failed to finalize the ACME order")?;
    let cert_pem = order
        .poll_certificate(&RetryPolicy::default())
        .await
        .context("failed to download the issued certificate")?;

    // Write before installing: a certificate that is serving traffic but was
    // never stored would be lost on restart.
    let cert_path = storage.join("cert.pem");
    let key_path = storage.join("key.pem");
    write_private(&key_path, key_pem.as_bytes())?;
    std::fs::write(&cert_path, cert_pem.as_bytes())
        .with_context(|| format!("failed to write {}", cert_path.display()))?;

    resolver
        .install(
            cert_pem.as_bytes(),
            key_pem.as_bytes(),
            config.domains.clone(),
        )
        .context("the issued certificate could not be installed")?;

    Ok(certificate_expiry(cert_pem.as_bytes()).unwrap_or_else(|| now_epoch_s() + 90 * 86_400))
}

/// Reuses the stored account, or registers a new one and saves the credentials.
async fn load_or_create_account(config: &AcmeConfig, storage: &Path) -> anyhow::Result<Account> {
    let path = storage.join("account.json");

    if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let credentials: AccountCredentials =
            serde_json::from_str(&raw).context("stored ACME account credentials are not valid")?;
        let account = account_builder(config)?
            .from_credentials(credentials)
            .await
            .context("failed to restore the ACME account")?;
        return Ok(account);
    }

    let contact: Vec<String> = config
        .email
        .iter()
        .map(|email| format!("mailto:{email}"))
        .collect();
    let contact: Vec<&str> = contact.iter().map(String::as_str).collect();

    let (account, credentials) = account_builder(config)?
        .create(
            &NewAccount {
                contact: &contact,
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            config.directory_url.clone(),
            None,
        )
        .await
        .context("failed to register an ACME account")?;

    let serialized =
        serde_json::to_vec_pretty(&credentials).context("failed to serialize ACME credentials")?;
    write_private(&path, &serialized)?;
    info!(path = %path.display(), "registered a new ACME account");

    Ok(account)
}

/// Builds the ACME client, trusting a private CA when one is configured.
fn account_builder(config: &AcmeConfig) -> anyhow::Result<instant_acme::AccountBuilder> {
    match &config.ca_root_path {
        Some(path) => Account::builder_with_root(path)
            .with_context(|| format!("failed to read the ACME CA root {path}")),
        None => Account::builder().context("failed to build the ACME client"),
    }
}

/// Whether a new certificate is needed: none stored, unreadable, or inside the
/// renewal window.
fn certificate_due(storage: &Path, config: &AcmeConfig) -> anyhow::Result<bool> {
    let cert_path = storage.join("cert.pem");
    if !cert_path.exists() {
        return Ok(true);
    }
    let cert = std::fs::read(&cert_path)
        .with_context(|| format!("failed to read {}", cert_path.display()))?;
    let Some(expiry) = certificate_expiry(&cert) else {
        return Ok(true);
    };

    let renew_at = expiry - (config.renew_before_days as i64) * 86_400;
    Ok(now_epoch_s() >= renew_at)
}

/// Reads the expiry of the leaf certificate in a PEM chain.
pub fn certificate_expiry(cert_pem: &[u8]) -> Option<i64> {
    let chain = pingora::tls::x509::X509::stack_from_pem(cert_pem).ok()?;
    let leaf = chain.first()?;
    crate::tls::parse_not_after(&leaf.not_after().to_string())
}

/// Loads a certificate that a previous run obtained, so a restart serves
/// traffic immediately instead of waiting for a fresh order.
pub fn load_stored_certificate(
    config: &AcmeConfig,
    resolver: &CertResolver,
) -> anyhow::Result<bool> {
    let storage = PathBuf::from(&config.storage_dir);
    let cert_path = storage.join("cert.pem");
    let key_path = storage.join("key.pem");
    if !cert_path.exists() || !key_path.exists() {
        return Ok(false);
    }

    let cert = std::fs::read(&cert_path)?;
    let key = std::fs::read(&key_path)?;
    resolver.install(&cert, &key, config.domains.clone())?;
    Ok(true)
}

fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    restrict_permissions(path)
}

/// Account keys and private keys must not be world readable.
fn restrict_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.is_dir() { 0o700 } else { 0o600 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .with_context(|| format!("failed to restrict permissions on {}", path.display()))?;
    }
    Ok(())
}

fn record_attempt(status: &SharedStatus) {
    if let Ok(mut status) = status.write() {
        status.last_attempt_epoch_s = Some(now_epoch_s());
    }
}

fn record_success(status: &SharedStatus, expiry: i64) {
    if let Ok(mut status) = status.write() {
        let now = now_epoch_s();
        status.last_success_epoch_s = Some(now);
        status.last_error = None;
        status.certificate_expiry_epoch_s = Some(expiry);
    }
}

fn record_failure(status: &SharedStatus, error: String) {
    if let Ok(mut status) = status.write() {
        status.last_error = Some(error);
    }
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
    fn the_challenge_store_answers_only_known_tokens() {
        let store = ChallengeStore::new();

        store.insert("token-a".to_string(), "answer-a".to_string());
        assert_eq!(store.get("token-a").as_deref(), Some("answer-a"));
        assert_eq!(store.get("token-b"), None);

        store.remove("token-a");
        assert_eq!(store.get("token-a"), None);
    }

    #[test]
    fn the_challenge_prefix_is_owned_only_once_acme_is_enabled() {
        // Ownership must not depend on a challenge being in flight: between
        // orders the store is empty, and the prefix still belongs to prx.
        let store = ChallengeStore::new();
        assert!(!store.is_active());

        store.activate();
        assert!(store.is_active());
        assert_eq!(store.get("token-a"), None);
    }

    #[test]
    fn a_missing_certificate_is_due() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = AcmeConfig {
            storage_dir: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        assert!(certificate_due(dir.path(), &config).expect("checked"));
    }

    #[test]
    fn an_unreadable_certificate_is_due_rather_than_fatal() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("cert.pem"), b"not a certificate").expect("write");
        let config = AcmeConfig {
            storage_dir: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        assert!(certificate_due(dir.path(), &config).expect("checked"));
    }
}
