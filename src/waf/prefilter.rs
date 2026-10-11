//! Literal prefilter for WAF (T508).
//!
//! Extracts literal substrings from `@rx` operators and builds an Aho-Corasick
//! automaton to quickly skip non-matching rules.

use aho_corasick::AhoCorasick;

#[derive(Debug, Clone)]
pub struct Prefilter {
    pub automaton: AhoCorasick,
    pub literals: Vec<String>,
}

impl Prefilter {
    pub fn new(literals: Vec<String>) -> Self {
        let automaton =
            AhoCorasick::new(&literals).unwrap_or_else(|_| AhoCorasick::new(vec![""]).unwrap());
        Self {
            automaton,
            literals,
        }
    }

    pub fn is_match(&self, input: &[u8]) -> bool {
        self.automaton.is_match(input)
    }
}
