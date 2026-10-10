use std::borrow::Cow;
use std::sync::Arc;
use std::collections::BTreeMap;
use crate::waf::rule::{RuleSet, Rule, RuleLink, Key, Operator};
use pingora::http::RequestHeader;

pub struct Evaluator {
    ruleset: Arc<RuleSet>,
}

impl Evaluator {
    pub fn new(ruleset: Arc<RuleSet>) -> Self {
        Self { ruleset }
    }

    pub fn evaluate_request_head(&self, head: &RequestHeader, body: &[u8]) -> Option<String> {
        // Phase 1 is index 0
        self.evaluate_phase(0, head, body)
    }

    fn evaluate_phase(&self, phase_index: usize, head: &RequestHeader, body: &[u8]) -> Option<String> {
        for rule in &self.ruleset.phases[phase_index] {
            if self.evaluate_rule(rule, head, body) {
                return Some(format!("Rule {} matched", rule.id));
            }
        }
        None
    }

    fn evaluate_rule(&self, rule: &Rule, head: &RequestHeader, body: &[u8]) -> bool {
        for link in &rule.links {
            if !self.evaluate_link(link, head, body) {
                return false;
            }
        }
        true
    }

    fn evaluate_link(&self, link: &RuleLink, head: &RequestHeader, body: &[u8]) -> bool {
        let mut matched = false;
        for target in &link.targets {
            let extracted = self.extract_variable(&target.collection, &target.key, head, body);
            // Apply transformations
            let transformed = crate::waf::transform::apply_chain(&self.ruleset.transform_chains[link.transforms], extracted);
            
            // Evaluate operator
            if self.evaluate_operator(&link.operator, transformed) {
                matched = !target.exclude;
                break;
            } else if target.exclude {
            }
        }
        matched
    }

    fn extract_variable<'a>(&self, collection: &str, key: &Key, head: &'a RequestHeader, body: &'a [u8]) -> Cow<'a, [u8]> {
        match collection {
            "REQUEST_HEADERS" => {
                match key {
                    Key::All => {
                        let mut all = Vec::new();
                        for (name, value) in head.headers.iter() {
                            all.extend(name.as_str().as_bytes());
                            all.extend(b": ");
                            all.extend(value.as_bytes());
                            all.extend(b"\n");
                        }
                        Cow::Owned(all)
                    }
                    Key::Name(name) => {
                        if let Some(value) = head.headers.get(name) {
                            Cow::Borrowed(value.as_bytes())
                        } else {
                            Cow::Borrowed(b"")
                        }
                    }
                    Key::Regex(_) => Cow::Borrowed(b""), // Regex not implemented
                }
            }
            "REQUEST_BODY" => {
                match key {
                    Key::All => Cow::Borrowed(body),
                    _ => Cow::Borrowed(b""), // Not implemented for specific keys/regex
                }
            }
            _ => Cow::Borrowed(b""),
        }
    }

    fn evaluate_operator(&self, operator: &Operator, value: Cow<'_, [u8]>) -> bool {
        match operator.name.to_ascii_lowercase().as_str() {
            "eq" => {
                value.as_ref() == operator.argument.as_bytes()
            }
            // Add more operators as needed
            _ => false,
        }
    }
}
