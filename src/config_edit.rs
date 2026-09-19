//! Structured edits to a config file, keeping the file (T308).
//!
//! The Settings forms change one field at a time, and every change has to land
//! in the same draft the TOML editor is showing (T307) — which means changing
//! the *file*, not re-rendering it from a parsed model. A re-render loses every
//! comment in it, and a config file is mostly comments explaining why a number
//! is what it is.
//!
//! So the browser sends the draft plus what changed, and `toml_edit` rewrites
//! exactly the keys named, leaving the rest of the bytes alone.

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, Item, Table, Value, value};

/// What to do to one key.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditAction {
    /// Write `value` at `path`, creating the tables it needs.
    #[default]
    Set,
    /// Delete the key, or the array-of-tables element, at `path`.
    Remove,
    /// Push a new `[[path]]` element built from `value`.
    Append,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EditOp {
    /// Dotted path, with `[i]` for an array-of-tables element:
    /// `server.tls.acme.domains`, `service[0].upstream[1].weight`.
    pub path: String,
    #[serde(default)]
    pub action: EditAction,
    /// The new value, as JSON. Ignored by `remove`.
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

/// Applies edits in order, returning the rewritten file.
pub fn apply_edits(text: &str, ops: &[EditOp]) -> anyhow::Result<String> {
    let mut document: DocumentMut = text
        .parse()
        .context("the draft is not valid TOML, so it cannot be edited field by field")?;

    for op in ops {
        apply_one(&mut document, op)
            .with_context(|| format!("failed to apply the change to '{}'", op.path))?;
    }

    Ok(document.to_string())
}

/// One path segment: a key, and the array index that follows it if any.
struct Segment<'a> {
    key: &'a str,
    index: Option<usize>,
}

fn parse_path(path: &str) -> anyhow::Result<Vec<Segment<'_>>> {
    if path.trim().is_empty() {
        bail!("empty path");
    }

    let mut segments = Vec::new();
    for raw in path.split('.') {
        let (key, index) = match raw.find('[') {
            None => (raw, None),
            Some(open) => {
                let Some(close) = raw.find(']') else {
                    bail!("missing ']' in '{raw}'");
                };
                if close < open {
                    bail!("malformed index in '{raw}'");
                }
                let index: usize = raw[open + 1..close]
                    .parse()
                    .with_context(|| format!("'{raw}' has a non-numeric index"))?;
                (&raw[..open], Some(index))
            }
        };

        if key.is_empty() {
            bail!("empty key in '{path}'");
        }
        segments.push(Segment { key, index });
    }

    Ok(segments)
}

fn apply_one(document: &mut DocumentMut, op: &EditOp) -> anyhow::Result<()> {
    let segments = parse_path(&op.path)?;
    let (last, parents) = segments
        .split_last()
        .expect("parse_path rejects an empty path");

    let mut table = document.as_table_mut();
    for segment in parents {
        table = descend(table, segment)?;
    }

    match op.action {
        EditAction::Remove => remove(table, last),
        EditAction::Append => {
            let Some(payload) = op.value.as_ref() else {
                bail!("append needs a value");
            };
            append(table, last, payload)
        }
        EditAction::Set => {
            let Some(payload) = op.value.as_ref() else {
                // A field cleared in the UI arrives as null, and the right
                // answer to that is to take the key out rather than write
                // something that means "unset" in TOML, which has no such thing.
                return remove(table, last);
            };
            set(table, last, payload)
        }
    }
}

/// Walks into (or creates) the table a segment names.
fn descend<'t>(table: &'t mut Table, segment: &Segment<'_>) -> anyhow::Result<&'t mut Table> {
    match segment.index {
        Some(index) => {
            let Some(array) = table
                .get_mut(segment.key)
                .and_then(Item::as_array_of_tables_mut)
            else {
                bail!("'{}' is not an array of tables", segment.key);
            };
            array
                .get_mut(index)
                .with_context(|| format!("'{}[{index}]' does not exist", segment.key))
        }
        None => {
            if !table.contains_key(segment.key) {
                table.insert(segment.key, Item::Table(Table::new()));
            }
            table
                .get_mut(segment.key)
                .and_then(Item::as_table_mut)
                .with_context(|| format!("'{}' is not a table", segment.key))
        }
    }
}

fn remove(table: &mut Table, segment: &Segment<'_>) -> anyhow::Result<()> {
    match segment.index {
        Some(index) => {
            let Some(array) = table
                .get_mut(segment.key)
                .and_then(Item::as_array_of_tables_mut)
            else {
                bail!("'{}' is not an array of tables", segment.key);
            };
            if index >= array.len() {
                bail!("'{}[{index}]' does not exist", segment.key);
            }
            array.remove(index);
            // An array of tables with nothing left in it is noise in the file.
            if array.is_empty() {
                table.remove(segment.key);
            }
        }
        None => {
            table.remove(segment.key);
        }
    }
    Ok(())
}

fn set(
    table: &mut Table,
    segment: &Segment<'_>,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    if let Some(index) = segment.index {
        let Some(array) = table
            .get_mut(segment.key)
            .and_then(Item::as_array_of_tables_mut)
        else {
            bail!("'{}' is not an array of tables", segment.key);
        };
        let Some(element) = array.get_mut(index) else {
            bail!("'{}[{index}]' does not exist", segment.key);
        };
        *element = table_from_json(payload)?;
        return Ok(());
    }

    // Writing a whole table, e.g. replacing `[server.tls]` in one go.
    if payload.is_object() {
        let mut new_table = table_from_json(payload)?;
        if let Some(existing) = table.get(segment.key).and_then(Item::as_table) {
            // Keep the decoration the file already has, so rewriting a table
            // does not move it or drop the comment above it.
            *new_table.decor_mut() = existing.decor().clone();
            new_table.set_position(existing.position().unwrap_or_default());
        }
        table.insert(segment.key, Item::Table(new_table));
        return Ok(());
    }

    table[segment.key] = value(value_from_json(payload)?);
    Ok(())
}

fn append(
    table: &mut Table,
    segment: &Segment<'_>,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    if !payload.is_object() {
        bail!("append needs an object, e.g. a new [[server.tls.cert]]");
    }

    if !table.contains_key(segment.key) {
        table.insert(
            segment.key,
            Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }
    let Some(array) = table
        .get_mut(segment.key)
        .and_then(Item::as_array_of_tables_mut)
    else {
        bail!("'{}' is not an array of tables", segment.key);
    };

    array.push(table_from_json(payload)?);
    Ok(())
}

fn table_from_json(payload: &serde_json::Value) -> anyhow::Result<Table> {
    let Some(object) = payload.as_object() else {
        bail!("expected an object");
    };

    let mut table = Table::new();
    for (key, entry) in object {
        if entry.is_null() {
            continue;
        }
        if entry.is_object() {
            table.insert(key, Item::Table(table_from_json(entry)?));
        } else {
            table[key.as_str()] = value(value_from_json(entry)?);
        }
    }
    Ok(table)
}

fn value_from_json(payload: &serde_json::Value) -> anyhow::Result<Value> {
    Ok(match payload {
        serde_json::Value::Bool(flag) => Value::from(*flag),
        serde_json::Value::String(text) => Value::from(text.as_str()),
        serde_json::Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                Value::from(integer)
            } else if let Some(float) = number.as_f64() {
                Value::from(float)
            } else {
                bail!("number out of range: {number}");
            }
        }
        serde_json::Value::Array(items) => {
            let mut array = Array::new();
            for item in items {
                array.push(value_from_json(item)?);
            }
            // `["a", "b"]` rather than `["a","b"]`: the file is read by people.
            array.fmt();
            Value::Array(array)
        }
        serde_json::Value::Object(_) => {
            let mut inline = toml_edit::InlineTable::new();
            for (key, entry) in payload.as_object().expect("checked") {
                if entry.is_null() {
                    continue;
                }
                inline.insert(key, value_from_json(entry)?);
            }
            Value::InlineTable(inline)
        }
        serde_json::Value::Null => bail!("null has no TOML representation"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PrxConfig;

    const SAMPLE: &str = r#"# The proxy everything else hangs off.
[server]
# Two ports, because the load balancer in front needs both.
listen = ["0.0.0.0:8080"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "info"
access_log = true

[[service]]
name = "api"

[[service.upstream]]
addr = "127.0.0.1:3001"
weight = 1

[[route]]
name = "api"
service = "api"
path_prefix = "/"
is_default = true
"#;

    /// The smallest `[server.tls]` that passes validation.
    fn tls_table() -> serde_json::Value {
        serde_json::json!({
            "listen": "0.0.0.0:8443",
            "cert_path": "/etc/prx/tls.crt",
            "key_path": "/etc/prx/tls.key"
        })
    }

    fn edit(text: &str, path: &str, value: serde_json::Value) -> String {
        apply_edits(
            text,
            &[EditOp {
                path: path.to_string(),
                action: EditAction::Set,
                value: Some(value),
            }],
        )
        .expect("edit should apply")
    }

    #[test]
    fn a_changed_field_leaves_every_comment_in_place() {
        let edited = edit(SAMPLE, "observability.log_level", "debug".into());

        assert!(edited.contains("log_level = \"debug\""));
        assert!(
            edited.contains("# The proxy everything else hangs off."),
            "the comments are the reason for editing the file rather than \
             re-rendering it:\n{edited}"
        );
        assert!(edited.contains("# Two ports, because the load balancer in front needs both."));
        assert_eq!(
            edited.lines().count(),
            SAMPLE.lines().count(),
            "changing one value should not move anything else"
        );
    }

    #[test]
    fn a_missing_table_is_created_on_the_way_down() {
        // A TLS listener is only a valid config once it has a certificate, so
        // this is the shape the form sends: the whole table at once.
        let edited = edit(
            SAMPLE,
            "server.tls",
            serde_json::json!({
                "listen": "0.0.0.0:8443",
                "cert_path": "/etc/prx/tls.crt",
                "key_path": "/etc/prx/tls.key"
            }),
        );

        assert!(edited.contains("[server.tls]"), "{edited}");
        assert!(edited.contains("listen = \"0.0.0.0:8443\""));
        let parsed = PrxConfig::from_toml_str(&edited).expect("still a valid config");
        assert_eq!(
            parsed.server.tls.expect("tls").listen,
            "0.0.0.0:8443".to_string()
        );
    }

    #[test]
    fn a_nested_element_is_addressed_by_index() {
        let edited = edit(SAMPLE, "service[0].upstream[0].weight", 5.into());

        let parsed = PrxConfig::from_toml_str(&edited).expect("valid");
        assert_eq!(parsed.services[0].upstreams[0].weight, 5);
        assert!(edited.contains("addr = \"127.0.0.1:3001\""));
    }

    #[test]
    fn a_list_is_written_as_a_readable_array() {
        let edited = edit(
            SAMPLE,
            "server.listen",
            serde_json::json!(["0.0.0.0:8080", "0.0.0.0:8081"]),
        );

        assert!(
            edited.contains(r#"listen = ["0.0.0.0:8080", "0.0.0.0:8081"]"#),
            "{edited}"
        );
    }

    #[test]
    fn clearing_a_field_takes_the_key_out() {
        let with_threads = edit(SAMPLE, "server.threads", 4.into());
        assert!(with_threads.contains("threads = 4"));

        let cleared = apply_edits(
            &with_threads,
            &[EditOp {
                path: "server.threads".to_string(),
                action: EditAction::Set,
                value: None,
            }],
        )
        .expect("clearing should apply");

        assert!(!cleared.contains("threads"), "{cleared}");
        // Back to where it started, byte for byte.
        assert_eq!(cleared, SAMPLE);
    }

    #[test]
    fn a_whole_table_can_be_removed() {
        let with_tls = edit(SAMPLE, "server.tls", tls_table());

        let without = apply_edits(
            &with_tls,
            &[EditOp {
                path: "server.tls".to_string(),
                action: EditAction::Remove,
                value: None,
            }],
        )
        .expect("remove should apply");

        assert!(!without.contains("[server.tls]"), "{without}");
        assert!(PrxConfig::from_toml_str(&without).is_ok());
    }

    #[test]
    fn a_certificate_is_appended_as_its_own_block() {
        let edited = apply_edits(
            &edit(SAMPLE, "server.tls", tls_table()),
            &[EditOp {
                path: "server.tls.cert".to_string(),
                action: EditAction::Append,
                value: Some(serde_json::json!({
                    "domains": ["example.com"],
                    "cert_path": "/etc/prx/example.crt",
                    "key_path": "/etc/prx/example.key",
                    "is_default": true
                })),
            }],
        )
        .expect("append should apply");

        assert!(edited.contains("[[server.tls.cert]]"), "{edited}");
        let parsed = PrxConfig::from_toml_str(&edited).expect("valid");
        let certs = parsed.server.tls.expect("tls").certs;
        assert_eq!(certs.len(), 1);
        assert_eq!(certs[0].domains, vec!["example.com".to_string()]);
        assert!(certs[0].is_default);
    }

    #[test]
    fn removing_the_last_element_removes_the_array() {
        let with_cert = apply_edits(
            &edit(SAMPLE, "server.tls", tls_table()),
            &[EditOp {
                path: "server.tls.cert".to_string(),
                action: EditAction::Append,
                value: Some(serde_json::json!({
                    "cert_path": "/etc/prx/example.crt",
                    "key_path": "/etc/prx/example.key"
                })),
            }],
        )
        .expect("append");

        let without = apply_edits(
            &with_cert,
            &[EditOp {
                path: "server.tls.cert[0]".to_string(),
                action: EditAction::Remove,
                value: None,
            }],
        )
        .expect("remove");

        assert!(!without.contains("[[server.tls.cert]]"), "{without}");
    }

    #[test]
    fn several_edits_apply_in_order() {
        let edited = apply_edits(
            SAMPLE,
            &[
                EditOp {
                    path: "observability.access_log".to_string(),
                    action: EditAction::Set,
                    value: Some(false.into()),
                },
                EditOp {
                    path: "observability.prometheus_listen".to_string(),
                    action: EditAction::Set,
                    value: Some("127.0.0.1:9100".into()),
                },
            ],
        )
        .expect("edits should apply");

        let parsed = PrxConfig::from_toml_str(&edited).expect("valid");
        assert!(!parsed.observability.access_log);
        assert_eq!(
            parsed.observability.prometheus_listen.as_deref(),
            Some("127.0.0.1:9100")
        );
    }

    #[test]
    fn an_edit_that_cannot_be_placed_is_refused_rather_than_guessed() {
        let error = apply_edits(
            SAMPLE,
            &[EditOp {
                path: "service[9].name".to_string(),
                action: EditAction::Set,
                value: Some("nope".into()),
            }],
        )
        .expect_err("there is no tenth service");

        assert!(format!("{error:#}").contains("service[9]"), "{error:#}");
    }

    #[test]
    fn a_draft_that_is_not_toml_yet_is_refused_whole() {
        let error = apply_edits("[server]\nlisten = [\n", &[])
            .expect_err("a half-typed file cannot be patched");

        assert!(format!("{error:#}").contains("not valid TOML"), "{error:#}");
    }
}
