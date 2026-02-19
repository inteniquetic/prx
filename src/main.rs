mod config;
mod metrics;
mod proxy;
mod reload;
mod runtime;

use std::{env, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use arc_swap::ArcSwap;
use pingora::{listeners::tls::TlsSettings, prelude::*};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::{
    config::PrxConfig, proxy::PrxProxy, reload::spawn_config_watcher, runtime::RuntimeConfig,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let config_path = env::var("PRX_CONFIG").unwrap_or_else(|_| "Prx.toml".to_string());
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

    let mut proxy_service = http_proxy_service(
        &server.configuration,
        PrxProxy::new(
            runtime_config.clone(),
            app_config.observability.access_log,
            app_config.server.health_path.clone(),
            app_config.server.ready_path.clone(),
        ),
    );

    for addr in &app_config.server.listen {
        proxy_service.add_tcp(addr);
    }

    if let Some(tls) = &app_config.server.tls {
        let mut tls_settings = TlsSettings::intermediate(&tls.cert_path, &tls.key_path)
            .with_context(|| {
                format!(
                    "failed to initialize TLS settings using cert={} key={}",
                    tls.cert_path, tls.key_path
                )
            })?;
        if tls.enable_h2 {
            tls_settings.enable_h2();
        }
        proxy_service.add_tls_with_settings(&tls.listen, None, tls_settings);
    }

    server.add_service(proxy_service);

    spawn_config_watcher(
        config_path.clone(),
        Duration::from_millis(app_config.server.config_reload_debounce_ms.max(50)),
        runtime_config,
    )
    .with_context(|| {
        format!(
            "failed to start config watcher for {}",
            config_path.to_string_lossy()
        )
    })?;

    if let Some(metrics_addr) = &app_config.observability.prometheus_listen {
        let mut metrics_service = pingora::services::listening::Service::prometheus_http_service();
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
