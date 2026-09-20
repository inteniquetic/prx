//! Is the prefilter worth building? (T504)
//!
//! The plan claims a WAF can run CRS without running CRS's regexes: pull the
//! literal substrings out of every pattern, put them in one Aho-Corasick
//! automaton, and only run the regexes whose literals were present. This
//! measures that against the two honest alternatives — a naive loop, and the
//! `RegexSet` the regex crate already offers — on the same corpus of real
//! attack payloads.

#[path = "../translate.rs"]
mod translate;

use std::io::Read;
use std::time::Instant;

use aho_corasick::AhoCorasick;
use regex::{Regex, RegexSet};

/// Literal substrings that must be present for a pattern to have any chance of
/// matching. `None` means the pattern can match without any fixed substring,
/// so it has to run every time.
fn required_literals(pattern: &str) -> Option<Vec<String>> {
    let hir = regex_syntax::ParserBuilder::new()
        .build()
        .parse(pattern)
        .ok()?;
    let seq = regex_syntax::hir::literal::Extractor::new()
        .kind(regex_syntax::hir::literal::ExtractKind::Prefix)
        .extract(&hir);
    // An inexact or unbounded set cannot be used to rule a pattern out.
    let lits = seq.literals()?;
    if lits.is_empty() || !seq.is_exact() && lits.iter().any(|l| l.len() < 3) {
        return None;
    }
    let out: Vec<String> = lits
        .iter()
        .filter_map(|l| String::from_utf8(l.as_bytes().to_vec()).ok())
        .filter(|s| s.len() >= 3)
        .collect();
    (out.len() == lits.len() && !out.is_empty()).then_some(out)
}

fn main() {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let corpus: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string("/tmp/corpus.json").unwrap()).unwrap();

    let patterns: Vec<String> = doc["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["op"].as_str() == Some("rx"))
        .map(|r| translate::pcre_to_rust(r["arg"].as_str().unwrap_or("")))
        .collect();

    // --- compile ----------------------------------------------------------
    let started = Instant::now();
    let compiled: Vec<Regex> = patterns
        .iter()
        .map(|p| Regex::new(p).expect("all patterns compile after translation"))
        .collect();
    let compile_individual_ms = started.elapsed().as_millis();

    // One RegexSet over the whole of CRS does not fit under the crate's
    // default 10 MB program limit — which is itself an argument for the
    // per-target sets the plan proposed rather than one set for everything.
    let started = Instant::now();
    let set = regex::RegexSetBuilder::new(&patterns)
        .size_limit(256 * 1024 * 1024)
        .build()
        .expect("regex set with a raised size limit");
    let compile_set_ms = started.elapsed().as_millis();
    let set_fits_default_limit = RegexSet::new(&patterns).is_ok();

    // --- prefilter --------------------------------------------------------
    let started = Instant::now();
    let mut literals: Vec<String> = Vec::new();
    let mut gate: Vec<Option<(usize, usize)>> = Vec::new(); // range into `literals`
    let mut always_run = 0usize;
    for pattern in &patterns {
        match required_literals(pattern) {
            Some(lits) => {
                let start = literals.len();
                literals.extend(lits);
                gate.push(Some((start, literals.len())));
            }
            None => {
                gate.push(None);
                always_run += 1;
            }
        }
    }
    let automaton = AhoCorasick::new(&literals).expect("aho-corasick");
    let compile_prefilter_ms = started.elapsed().as_millis();

    // --- match ------------------------------------------------------------
    let started = Instant::now();
    let mut naive_hits = 0usize;
    for input in &corpus {
        for re in &compiled {
            if re.is_match(input) {
                naive_hits += 1;
            }
        }
    }
    let naive_us = started.elapsed().as_micros();

    let started = Instant::now();
    let mut set_hits = 0usize;
    for input in &corpus {
        set_hits += set.matches(input).iter().count();
    }
    let set_us = started.elapsed().as_micros();

    let started = Instant::now();
    let (mut pre_hits, mut regexes_run) = (0usize, 0usize);
    for input in &corpus {
        let mut present = vec![false; literals.len()];
        for m in automaton.find_overlapping_iter(input) {
            present[m.pattern().as_usize()] = true;
        }
        for (index, re) in compiled.iter().enumerate() {
            let runnable = match gate[index] {
                None => true,
                Some((a, b)) => present[a..b].iter().any(|p| *p),
            };
            if runnable {
                regexes_run += 1;
                if re.is_match(input) {
                    pre_hits += 1;
                }
            }
        }
    }
    let prefilter_us = started.elapsed().as_micros();

    let evaluations = corpus.len() * patterns.len();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "patterns": patterns.len(),
            "corpus": corpus.len(),
            "regex_set_fits_default_10mb_limit": set_fits_default_limit,
            "compile_ms": {
                "individual": compile_individual_ms,
                "regex_set": compile_set_ms,
                "prefilter_build": compile_prefilter_ms,
            },
            "prefilter": {
                "literals": literals.len(),
                "patterns_with_no_usable_literal": always_run,
            },
            "match": {
                "naive_us": naive_us,
                "regex_set_us": set_us,
                "prefilter_us": prefilter_us,
                "naive_us_per_request": naive_us as f64 / corpus.len() as f64,
                "regex_set_us_per_request": set_us as f64 / corpus.len() as f64,
                "prefilter_us_per_request": prefilter_us as f64 / corpus.len() as f64,
            },
            "regexes_run": {
                "naive": evaluations,
                "with_prefilter": regexes_run,
                "skipped_pct": 100.0 * (evaluations - regexes_run) as f64 / evaluations as f64,
            },
            "agreement": {
                "naive_hits": naive_hits,
                "regex_set_hits": set_hits,
                "prefilter_hits": pre_hits,
            },
        }))
        .unwrap()
    );
}
