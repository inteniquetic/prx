//! Loading the real OWASP CRS with each candidate engine (T504).
//!
//! The question is not whether a crate builds — both do — but whether it can
//! take CRS as shipped and say how much of it survived. A crate that loads 60%
//! of a rule set and reports success is worse than one that refuses, so what
//! is counted here is rules accepted, errors reported, and how long it took.

use std::time::Instant;

const CRS: &str = "/home/user/coreruleset/coreruleset";

/// CRS as one unit, which is how an `Include` chain delivers it.
///
/// Loading each file on its own looks tidier and is wrong: `skipAfter` and
/// SecMarker cross file boundaries, so a per-file loader reports errors the
/// real deployment never sees.
fn whole_ruleset() -> String {
    rule_files()
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn rule_files() -> Vec<std::path::PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(format!("{CRS}/rules"))
        .expect("CRS rules directory")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "conf"))
        .collect();
    files.sort();
    files
}

fn zentinel() -> serde_json::Value {
    use zentinel_modsec::CompiledRuleset;

    let source = whole_ruleset();
    // `@pmFromFile scanners-user-agents.data` is resolved relative to the
    // process's working directory, so the loader has to be run from where CRS
    // keeps its data files.
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(format!("{CRS}/rules")).unwrap();
    let started = Instant::now();
    let (loaded, refused) = match CompiledRuleset::from_string(&source) {
        Ok(rules) => (rules.rule_count(), None),
        Err(err) => (0, Some(err.to_string())),
    };
    let elapsed = started.elapsed();
    std::env::set_current_dir(previous).unwrap();
    serde_json::json!({
        "engine": "zentinel-modsec 0.4.0",
        "rules_loaded": loaded,
        "load_ms": elapsed.as_millis(),
        "refused": refused,
    })
}

fn barbacane() -> serde_json::Value {
    // The crate publishes its library as `parapet`.
    use parapet::engine::RuleSet;
    use parapet::matcher::DirDataLoader;
    use parapet::parse;

    let loader = DirDataLoader::new(format!("{CRS}/rules"));
    let source = whole_ruleset();
    let started = Instant::now();
    let mut samples: Vec<String> = Vec::new();
    let (directives, perrs) = parse::parse_all(&source, "crs");
    let parse_errors = perrs.len();
    for e in perrs.iter().take(3) {
        samples.push(format!("parse: {e}"));
    }
    let (set, cerrs) = RuleSet::compile_all(&directives, &loader);
    let compile_errors = cerrs.len();
    for e in cerrs.iter().take(3) {
        samples.push(format!("compile: {e}"));
    }
    let loaded = set.rule_count();
    serde_json::json!({
        "engine": "barbacane-waf 0.1.1",
        "rules_loaded": loaded,
        "parse_errors": parse_errors,
        "compile_errors": compile_errors,
        "load_ms": started.elapsed().as_millis(),
        "errors": samples,
    })
}

fn main() {
    let report = serde_json::json!({
        "crs": "v4.21.0",
        "secrules_in_corpus": 678,
        "engines": [zentinel(), barbacane()],
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
