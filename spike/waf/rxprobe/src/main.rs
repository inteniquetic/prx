//! Does OWASP CRS compile under Rust's `regex`? (T504)
//!
//! One number decides the rest of Phase 5, so this does the least it can:
//! feed every @rx pattern to the engine prx would use and record what comes
//! back. No interpretation here — that belongs in the decision record.

mod translate;

use std::collections::BTreeMap;
use std::io::Read;

fn classify(err: &str) -> &'static str {
    let e = err.to_ascii_lowercase();
    if e.contains("look-around") || e.contains("lookahead") || e.contains("lookbehind") {
        "lookaround"
    } else if e.contains("backreference") {
        "backreference"
    } else if e.contains("repetition quantifier expects") || e.contains("repetition operator") {
        "repetition"
    } else if e.contains("compiled regex exceeds size limit") {
        "size limit"
    } else if e.contains("unclosed") || e.contains("unopened") {
        "unbalanced"
    } else if e.contains("class") {
        "character class"
    } else if e.contains("escape") {
        "escape sequence"
    } else {
        "other"
    }
}

fn main() {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let rules = doc["rules"].as_array().unwrap();

    let mut ok = 0usize;
    let mut failures: Vec<(String, String, Option<u64>, String, String)> = Vec::new();
    let mut ok_by_pl: BTreeMap<String, usize> = BTreeMap::new();
    let mut fail_by_pl: BTreeMap<String, usize> = BTreeMap::new();

    for rule in rules {
        if rule["op"].as_str() != Some("rx") {
            continue;
        }
        let pattern = rule["arg"].as_str().unwrap_or("");
        let id = rule["id"].as_str().unwrap_or("?").to_string();
        let file = rule["file"].as_str().unwrap_or("?").to_string();
        let pl = rule["pl"].as_u64();
        let pl_key = pl.map(|v| format!("PL{v}")).unwrap_or_else(|| "none".into());

        let translated = std::env::args().any(|a| a == "--translate");
        let candidate = if translated {
            translate::pcre_to_rust(pattern)
        } else {
            pattern.to_string()
        };
        match regex::Regex::new(&candidate) {
            Ok(_) => {
                ok += 1;
                *ok_by_pl.entry(pl_key).or_default() += 1;
            }
            Err(err) => {
                let text = err.to_string();
                failures.push((id, file, pl, classify(&text).to_string(), text));
                *fail_by_pl.entry(pl_key).or_default() += 1;
            }
        }
    }

    let total = ok + failures.len();
    let out = serde_json::json!({
        "total_rx": total,
        "compiled": ok,
        "failed": failures.len(),
        "ok_by_pl": ok_by_pl,
        "fail_by_pl": fail_by_pl,
        "failures": failures.iter().map(|(id, file, pl, kind, msg)| serde_json::json!({
            "id": id, "file": file, "pl": pl, "kind": kind, "error": msg,
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
