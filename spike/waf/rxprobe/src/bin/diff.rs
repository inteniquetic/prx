//! Does the translation change what a pattern matches? (T504)
//!
//! Compiling is not meaning. For every CRS pattern that `regex` already
//! accepts untranslated, both forms are run over the same corpus of real
//! attack payloads — lifted from CRS's own regression tests — and any string
//! they disagree about is a bug in the translation, not a curiosity.

#[path = "../translate.rs"]
mod translate;

use std::io::Read;

fn main() {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let rules = doc["rules"].as_array().unwrap();
    let corpus: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string("/tmp/corpus.json").unwrap()).unwrap();

    let (mut comparable, mut changed, mut disagreements) = (0usize, 0usize, 0usize);
    let mut examples: Vec<String> = Vec::new();

    for rule in rules {
        if rule["op"].as_str() != Some("rx") {
            continue;
        }
        let pattern = rule["arg"].as_str().unwrap_or("");
        let id = rule["id"].as_str().unwrap_or("?");
        let translated = translate::pcre_to_rust(pattern);
        if translated != pattern {
            changed += 1;
        }
        // Only patterns the engine accepts both ways can be compared at all.
        let (Ok(before), Ok(after)) = (regex::Regex::new(pattern), regex::Regex::new(&translated))
        else {
            continue;
        };
        comparable += 1;
        for input in &corpus {
            if before.is_match(input) != after.is_match(input) {
                disagreements += 1;
                if examples.len() < 10 {
                    let cut = input.char_indices().nth(80).map(|(i, _)| i).unwrap_or(input.len());
                    examples.push(format!("rule {id}: {:?}", &input[..cut]));
                }
            }
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "patterns_compared": comparable,
            "patterns_changed_by_translation": changed,
            "corpus_size": corpus.len(),
            "comparisons": comparable * corpus.len(),
            "disagreements": disagreements,
            "examples": examples,
        }))
        .unwrap()
    );
}
