use std::{env, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use arc_swap::ArcSwap;
use pingora::{apps::HttpServerOptions, listeners::tls::TlsSettings, prelude::*};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use prx::{
    acme::{AcmeStatus, ChallengeStore, load_stored_certificate, spawn_acme},
    admin::{AdminAxumService, DEFAULT_ADMIN_LISTEN, bind_admin_listener},
    config::PrxConfig,
    health::spawn_health_checker,
    proxy::PrxProxy,
    reload::spawn_config_watcher,
    runtime::RuntimeConfig,
    tls::{CertResolver, ResolverHandle},
};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let config_path = env::var("PRX_CONFIG")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Prx.toml".to_string());
    let config_path = PathBuf::from(config_path);
    let app_config = PrxConfig::from_file(&config_path)?;
    init_tracing(&app_config.observability.log_level);

    let mut server =
        Server::new(Some(Opt::parse_args())).context("failed to initialize pingora server")?;
    tune_pingora_server(&mut server, &app_config);
    server.bootstrap();

    let runtime_config = Arc::new(ArcSwap::from_pointee(RuntimeConfig::from_config(
        app_config.clone(),
    )));

    // Shared with the ACME task so the plaintext listener can answer HTTP-01
    // challenges for the TLS listener's certificates.
    let acme_challenges = Arc::new(ChallengeStore::new());
    let acme_status: Arc<std::sync::RwLock<AcmeStatus>> = Arc::default();

    let mut proxy_service = http_proxy_service(
        &server.configuration,
        PrxProxy::new(
            runtime_config.clone(),
            app_config.observability.access_log,
            app_config.server.health_path.clone(),
            app_config.server.ready_path.clone(),
            app_config.compression.clone(),
            acme_challenges.clone(),
        ),
    );

    if app_config.server.h2c {
        // Pingora peeks for the h2 preface and falls back to HTTP/1.1, so this
        // costs nothing for HTTP/1.1 clients and is what cleartext gRPC needs.
        //
        // It must not be set on the TLS service: a TLS stream cannot be peeked,
        // so pingora would treat every connection as h2 even when ALPN settled
        // on http/1.1, and HTTP/1.1 clients would get an h2 frame as their
        // response body.
        if let Some(app) = proxy_service.app_logic_mut() {
            let mut options = HttpServerOptions::default();
            options.h2c = true;
            app.server_options = Some(options);
        }
    }

    for addr in &app_config.server.listen {
        proxy_service.add_tcp(addr);
    }

    let mut tls_resolver: Option<Arc<CertResolver>> = None;
    let tls_service = match &app_config.server.tls {
        Some(tls) => {
            // Certificates are loaded and checked here rather than during a
            // handshake: a bad path or a key that does not match its
            // certificate should stop startup, not break every client.
            let configured = tls.all_certs();
            let resolver = Arc::new(if configured.is_empty() {
                // Nothing on disk: ACME will install the first certificate.
                CertResolver::empty()
            } else {
                CertResolver::load(&configured)
                    .context("failed to load the configured TLS certificates")?
            });

            if tls.acme.enabled {
                // From here on prx answers the challenge prefix itself.
                acme_challenges.activate();

                // A certificate from a previous run means this restart serves
                // traffic immediately instead of waiting for a fresh order.
                match load_stored_certificate(&tls.acme, &resolver) {
                    Ok(true) => info!("loaded the stored ACME certificate"),
                    Ok(false) => info!("no stored ACME certificate yet; one will be ordered"),
                    Err(err) => warn!(error = %err, "could not load the stored ACME certificate"),
                }

                if let Ok(mut status) = acme_status.write() {
                    status.enabled = true;
                    status.directory_url = tls.acme.directory_url.clone();
                    status.domains = tls.acme.domains.clone();
                    status.staging = tls.acme.directory_url.contains("staging");
                }

                spawn_acme(
                    tls.acme.clone(),
                    resolver.clone(),
                    acme_challenges.clone(),
                    acme_status.clone(),
                )
                .context("failed to start the ACME task")?;
                info!(
                    domains = tls.acme.domains.join(",").as_str(),
                    directory = tls.acme.directory_url.as_str(),
                    "automatic certificates are enabled"
                );
            }

            resolver.report_expiry();
            let cert_count = resolver.len();
            tls_resolver = Some(resolver.clone());

            let mut tls_settings =
                TlsSettings::with_callbacks(Box::new(ResolverHandle(resolver.clone())))
                    .map_err(|err| anyhow::anyhow!("failed to initialize TLS settings: {err}"))?;
            if tls.enable_h2 {
                tls_settings.enable_h2();
            }

            // A separate service, so the h2c option above never applies to a
            // TLS listener.
            let mut service = http_proxy_service(
                &server.configuration,
                PrxProxy::new(
                    runtime_config.clone(),
                    app_config.observability.access_log,
                    app_config.server.health_path.clone(),
                    app_config.server.ready_path.clone(),
                    app_config.compression.clone(),
                    acme_challenges.clone(),
                ),
            );
            service.add_tls_with_settings(&tls.listen, None, tls_settings);
            info!(
                listen = tls.listen.as_str(),
                certificates = cert_count,
                "TLS listener is enabled"
            );
            Some(service)
        }
        None => None,
    };

    let proxy_listen = app_config.server.listen.join(", ");
    let tls_listen = app_config
        .server
        .tls
        .as_ref()
        .map(|tls| tls.listen.as_str())
        .unwrap_or("-");
    server.add_service(proxy_service);
    if let Some(tls_service) = tls_service {
        server.add_service(tls_service);
    }
    info!(
        listen = proxy_listen.as_str(),
        tls_listen, "proxy server listeners are enabled"
    );

    let admin_listen = env::var("PRX_ADMIN_LISTEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_ADMIN_LISTEN.to_string());
    let admin_listener = bind_admin_listener(&admin_listen)
        .with_context(|| format!("failed to start admin server on {admin_listen}"))?;
    server.add_service(AdminAxumService::new(
        admin_listen.clone(),
        admin_listener,
        config_path.clone(),
        runtime_config.clone(),
        acme_status.clone(),
        tls_resolver.clone(),
    ));
    spawn_config_watcher(
        config_path.clone(),
        Duration::from_millis(app_config.server.config_reload_debounce_ms.max(50)),
        runtime_config.clone(),
    )
    .with_context(|| {
        format!(
            "failed to start config watcher for {}",
            config_path.to_string_lossy()
        )
    })?;

    if app_config
        .services
        .iter()
        .any(|service| service.health_check.enabled)
    {
        spawn_health_checker(runtime_config.clone())
            .context("failed to start the active health checker")?;
        info!("active health checking is enabled");
    }

    if let Some(metrics_addr) = &app_config.observability.prometheus_listen {
        let mut metrics_service = pingora_prometheus::prometheus_http_service();
        metrics_service.add_tcp(metrics_addr);
        server.add_service(metrics_service);
        info!(
            listen = metrics_addr,
            "prometheus metrics endpoint is enabled"
        );
    }

    info!(
        config = %config_path.to_string_lossy(),
        "prx is starting"
    );
    server.run_forever();
}

fn init_tracing(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(level))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .compact()
        .init();
}

fn tune_pingora_server(server: &mut Server, app_config: &PrxConfig) {
    if let Some(conf) = Arc::get_mut(&mut server.configuration) {
        if let Some(threads) = app_config.server.threads {
            conf.threads = threads;
        }
        if let Some(seconds) = app_config.server.grace_period_seconds {
            conf.grace_period_seconds = Some(seconds);
        }
        if let Some(seconds) = app_config.server.graceful_shutdown_timeout_seconds {
            conf.graceful_shutdown_timeout_seconds = Some(seconds);
        }
    }
}
