use std::borrow::Cow;
use std::sync::Arc;
use std::collections::BTreeMap;
use crate::waf::rule::{RuleSet, Rule, RuleLink, Key, Operator, Rx};
use crate::waf::vars::Transaction;
use pingora::http::RequestHeader;

pub struct Evaluator {
    ruleset: Arc<RuleSet>,
}

impl Evaluator {
    pub fn new(ruleset: Arc<RuleSet>) -> Self {
        Self { ruleset }
    }

    pub fn evaluate_request_head<'a>(&'a self, trx: &'a Transaction<'a>) -> Option<String> {
        self.evaluate_phase(0, trx)
    }

    fn evaluate_phase<'a>(&'a self, phase_index: usize, trx: &'a Transaction<'a>) -> Option<String> {
        for rule in &self.ruleset.phases[phase_index] {
            if self.evaluate_rule(rule, trx) {
                return Some(format!("Rule {} matched", rule.id));
            }
        }
        None                
    }

    fn evaluate_rule<'a>(&self, rule: &'a Rule, trx: &'a Transaction<'a>) -> bool {
        for link in &rule.links {
            if !self.evaluate_link(link, trx) {
                return false;
            }
        }
        true
    }

    fn evaluate_link<'a>(&self, link: &'a RuleLink, trx: &'a Transaction<'a>) -> bool {
        let mut matched = false;
        for target in &link.targets {
            // Use the caching extractor
            let extracted = crate::waf::vars::get_variable(trx, &target.collection, &target.key);
            // Apply transformations
            let transformed = crate::waf::transform::apply_chain(&self.ruleset.transform_chains[link.transforms], extracted);
            
            // Evaluate operator
            if self.evaluate_operator(&link.operator, link.rx.as_ref(), transformed) {
                matched = !target.exclude;
                break;
            } else if target.exclude {
            }
        }
        matched
    }

    fn evaluate_operator(&self, operator: &Operator, rx: Option<&Rx>, value: Cow<'_, [u8]>) -> bool {
        // Need to provide macro resolver, captures, data_dir
        // For now, this is a placeholder or I need to pass down more context.
        crate::waf::operator::evaluate_operator(
            operator,
            rx,
            value.as_ref(),
            |_| None,
            &mut [None, None, None, None, None, None, None, None, None, None],
            None
        )
    }
}
