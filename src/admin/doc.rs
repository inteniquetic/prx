//! The admin API as documents: an OpenAPI description and a JSON Schema of
//! `Prx.toml` (T205).
//!
//! Neither is written by hand. Every schema comes from the Rust type through
//! `schemars`; `docs/openapi.json` and `docs/prx.schema.json` are what
//! `cargo run --example gen_api` prints, the WebUI's `api.d.ts` is generated
//! from the first, and a test fails when any of them is stale. The only
//! hand-kept part is which route takes and returns which type, in [`OPS`],
//! and a second test proves that table against the real router.

use schemars::{JsonSchema, SchemaGenerator, generate::SchemaSettings};
use serde_json::{Value, json};

use super::*;

type SchemaFn = fn(&mut SchemaGenerator) -> Value;

fn of<T: JsonSchema>(generator: &mut SchemaGenerator) -> Value {
    generator.subschema_for::<T>().to_value()
}

fn list_of<T: JsonSchema>(generator: &mut SchemaGenerator) -> Value {
    json!({ "type": "array", "items": generator.subschema_for::<T>().to_value() })
}

/// One operation: method, path, summary, request body, response body.
struct Op {
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    request: Option<SchemaFn>,
    response: Option<SchemaFn>,
}

const fn op(
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    request: Option<SchemaFn>,
    response: Option<SchemaFn>,
) -> Op {
    Op {
        method,
        path,
        summary,
        request,
        response,
    }
}

const OPS: &[Op] = &[
    op(
        "get",
        ADMIN_CONFIG_PATH,
        "The running config (JSON, or TOML with ?format=toml)",
        None,
        Some(of::<AdminConfigPayload>),
    ),
    op(
        "put",
        ADMIN_CONFIG_PATH,
        "Replace the config with TOML text; If-Match guards against a lost update",
        None,
        Some(of::<ValidateResponse>),
    ),
    op(
        "post",
        ADMIN_CONFIG_VALIDATE_PATH,
        "Validate TOML text without applying it",
        None,
        Some(of::<ValidateResponse>),
    ),
    op(
        "post",
        ADMIN_CONFIG_EDIT_PATH,
        "Apply targeted key edits, keeping comments and layout",
        Some(of::<EditRequest>),
        Some(of::<EditResponse>),
    ),
    op(
        "get",
        ADMIN_CONFIG_HISTORY_PATH,
        "Stored config versions, newest first",
        None,
        Some(list_of::<crate::history::Version>),
    ),
    op(
        "get",
        ADMIN_CONFIG_HISTORY_ID_PATH,
        "The TOML of one stored version",
        None,
        None,
    ),
    op(
        "post",
        ADMIN_CONFIG_ROLLBACK_PATH,
        "Re-apply a stored version after validating it",
        None,
        None,
    ),
    op(
        "get",
        ADMIN_AUDIT_PATH,
        "Recent audit log entries",
        None,
        None,
    ),
    op(
        "get",
        ADMIN_SERVICES_PATH,
        "List services",
        None,
        Some(list_of::<AdminServicePayload>),
    ),
    op(
        "post",
        ADMIN_SERVICES_PATH,
        "Create a service",
        Some(of::<ServiceRequestPayload>),
        Some(of::<AdminServicePayload>),
    ),
    op(
        "get",
        ADMIN_SERVICES_NAME_PATH,
        "One service",
        None,
        Some(of::<AdminServicePayload>),
    ),
    op(
        "put",
        ADMIN_SERVICES_NAME_PATH,
        "Update a service",
        Some(of::<ServiceRequestPayload>),
        Some(of::<AdminServicePayload>),
    ),
    op(
        "delete",
        ADMIN_SERVICES_NAME_PATH,
        "Delete a service",
        None,
        None,
    ),
    op(
        "get",
        ADMIN_ROUTES_PATH,
        "List routes",
        None,
        Some(list_of::<AdminRoutePayload>),
    ),
    op(
        "post",
        ADMIN_ROUTES_PATH,
        "Create a route",
        Some(of::<RouteRequestPayload>),
        Some(of::<AdminRoutePayload>),
    ),
    op(
        "get",
        ADMIN_ROUTES_NAME_PATH,
        "One route",
        None,
        Some(of::<AdminRoutePayload>),
    ),
    op(
        "put",
        ADMIN_ROUTES_NAME_PATH,
        "Update a route",
        Some(of::<RouteRequestPayload>),
        Some(of::<AdminRoutePayload>),
    ),
    op(
        "delete",
        ADMIN_ROUTES_NAME_PATH,
        "Delete a route",
        None,
        None,
    ),
    op(
        "post",
        ADMIN_ROUTE_TEST_PATH,
        "Which route and upstream a request would take",
        Some(of::<RouteTestRequest>),
        Some(of::<RouteTestResponse>),
    ),
    op(
        "get",
        ADMIN_ROUTE_HEALTH_PATH,
        "Last route health result",
        None,
        Some(of::<RouteHealthPayload>),
    ),
    op(
        "post",
        ADMIN_ROUTE_HEALTH_PATH,
        "Probe every upstream now",
        None,
        Some(of::<RouteHealthPayload>),
    ),
    op(
        "post",
        ADMIN_UPSTREAM_TEST_PATH,
        "Probe one upstream address",
        Some(of::<UpstreamTestRequest>),
        None,
    ),
    op(
        "get",
        ADMIN_SERVICE_STATUS_PATH,
        "Live upstream state per service",
        None,
        Some(of::<ServiceStatusPayload>),
    ),
    op(
        "get",
        ADMIN_STATS_PATH,
        "Current statistics snapshot",
        None,
        Some(of::<crate::stats::StatsSnapshot>),
    ),
    op(
        "get",
        ADMIN_STATS_STREAM_PATH,
        "Statistics as a server-sent event stream",
        None,
        None,
    ),
    op(
        "get",
        ADMIN_CACHE_PATH,
        "Cache status per route",
        None,
        Some(list_of::<CacheRoutePayload>),
    ),
    op(
        "delete",
        ADMIN_CACHE_PATH,
        "Purge a route's cache",
        None,
        None,
    ),
    op(
        "get",
        ADMIN_TLS_STATUS_PATH,
        "Certificates and ACME state",
        None,
        Some(of::<TlsStatusPayload>),
    ),
    op(
        "post",
        ADMIN_TLS_ACME_RENEW_PATH,
        "Ask for an ACME order now",
        None,
        None,
    ),
    op(
        "get",
        ADMIN_SCHEMA_PATH,
        "JSON Schema of Prx.toml",
        None,
        None,
    ),
];

/// JSON Schema (draft 2020-12) of the whole `Prx.toml`.
pub fn config_schema() -> Value {
    let generator = SchemaSettings::draft2020_12().into_generator();
    generator.into_root_schema_for::<PrxConfig>().to_value()
}

/// The OpenAPI 3.1 description of the admin API.
pub fn openapi() -> Value {
    // OpenAPI 3.1 is JSON Schema 2020-12, so an `Option<T>` is a plain
    // `anyOf [T, {type: null}]`; the 3.0 `nullable` flag is not valid there
    // without a `type`.
    let mut generator = SchemaSettings::draft2020_12()
        .with(|settings| settings.definitions_path = "/components/schemas".into())
        .into_generator();
    let mut paths = serde_json::Map::new();
    for op in OPS {
        let mut operation = serde_json::Map::new();
        operation.insert("summary".into(), json!(op.summary));
        let id: String = format!("{}_{}", op.method, op.path)
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        operation.insert("operationId".into(), json!(id));
        let params: Vec<Value> = op
            .path
            .split('/')
            .filter_map(|seg| seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
            .map(|name| {
                json!({ "name": name, "in": "path", "required": true, "schema": { "type": "string" } })
            })
            .collect();
        if !params.is_empty() {
            operation.insert("parameters".into(), json!(params));
        }
        if let Some(request) = op.request {
            operation.insert(
                "requestBody".into(),
                json!({ "required": true, "content": { "application/json": { "schema": request(&mut generator) } } }),
            );
        } else if op.method == "put" || op.method == "post" {
            operation.insert(
                "requestBody".into(),
                json!({ "content": { "text/plain": { "schema": { "type": "string" } } } }),
            );
        }
        let mut ok = json!({ "description": "OK" });
        if let Some(response) = op.response {
            ok["content"] = json!({ "application/json": { "schema": response(&mut generator) } });
        }
        operation.insert(
            "responses".into(),
            json!({
                "200": ok,
                "401": { "description": "Missing or wrong credential" },
                "403": { "description": "read_only, or a cross-origin write" },
            }),
        );
        let entry = paths
            .entry(op.path.to_string())
            .or_insert_with(|| json!({}));
        entry[op.method] = Value::Object(operation);
    }
    // Referenced by the schemas above and by nothing in OPS directly.
    of::<ConfigConflict>(&mut generator);
    of::<crate::validate::ValidationReport>(&mut generator);
    of::<PrxConfig>(&mut generator);
    let mut schemas = generator.take_definitions(true);
    let sorted: std::collections::BTreeMap<_, _> =
        std::mem::take(&mut schemas).into_iter().collect();
    json!({
        "openapi": "3.1.0",
        "servers": [{ "url": "/" }],
        "info": {
            "title": "prx admin API",
            "description": "Generated by `cargo run --example gen_api`; do not edit.",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "paths": paths,
        "components": {
            "schemas": sorted,
            "securitySchemes": {
                "bearer": { "type": "http", "scheme": "bearer" },
            },
        },
        "security": [{ "bearer": [] }],
    })
}

pub fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("a Value always serializes");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn on_disk(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    #[test]
    fn generated_documents_are_up_to_date() {
        assert!(
            on_disk("openapi.json") == pretty(&openapi()),
            "docs/openapi.json is stale: run `make schema`"
        );
        assert!(
            on_disk("prx.schema.json") == pretty(&config_schema()),
            "docs/prx.schema.json is stale: run `make schema`"
        );
    }

    #[test]
    fn every_admin_path_constant_is_documented() {
        let source = include_str!("../admin.rs");
        let documented: Vec<&str> = OPS.iter().map(|op| op.path).collect();
        for line in source.lines() {
            let Some(rest) = line.strip_prefix("pub const ADMIN_") else {
                continue;
            };
            let Some((_, value)) = rest.split_once("&str = \"") else {
                continue;
            };
            let path = value.trim_end_matches("\";");
            assert!(
                documented.contains(&path),
                "{path} is routed but missing from the OpenAPI table"
            );
        }
    }

    #[tokio::test]
    async fn every_documented_operation_is_routed() {
        use tower::ServiceExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Prx.toml");
        std::fs::write(
            &path,
            "[server]\nlisten = [\"127.0.0.1:0\"]\n[[service]]\nname = \"s\"\n[[service.upstream]]\naddr = \"127.0.0.1:9\"\n[[route]]\nname = \"r\"\nservice = \"s\"\nis_default = true\n",
        )
        .unwrap();
        let state = AdminState {
            config_admin: ConfigAdmin::new(path.clone()),
            active_config: Arc::new(ArcSwap::from_pointee(RuntimeConfig::from_config(
                PrxConfig::from_file(&path).unwrap(),
            ))),
            auth: AdminAuthState::None,
            policy: AdminPolicy::from_config(&crate::config::AdminConfig::default()),
            acme_status: Default::default(),
            tls_resolver: None,
            acme_renew: None,
        };
        for op in OPS {
            let uri = op.path.replace("{name}", "x").replace("{id}", "x");
            let request = Request::builder()
                .method(op.method.to_ascii_uppercase().as_str())
                .uri(uri.as_str())
                .header("host", "127.0.0.1")
                .body(Body::empty())
                .unwrap();
            let response = build_router(state.clone()).oneshot(request).await.unwrap();
            assert_ne!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{} {} is documented but not routed",
                op.method,
                op.path
            );
        }
    }

    #[tokio::test]
    async fn the_schema_endpoint_serves_the_generated_schema() {
        let response = get_schema().await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let served: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(served, config_schema());
        assert!(served["properties"]["route"].is_object());
    }
}
