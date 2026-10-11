use crate::waf::rule::{Operator, Rule, RuleLink, RuleSet, Rx};
use crate::waf::vars::Transaction;
use std::borrow::Cow;
use std::sync::Arc;

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

    fn evaluate_phase<'a>(
        &'a self,
        phase_index: usize,
        trx: &'a Transaction<'a>,
    ) -> Option<String> {
        for rule in &self.ruleset.phases[phase_index] {
            if self.evaluate_rule(rule, trx) {
                // Rule matched, trigger actions
                self.process_actions(rule, trx);
                return Some(format!("Rule {} matched", rule.id));
            }
        }
        None
    }

    fn process_actions(&self, rule: &Rule, trx: &Transaction) {
        // Actions are on the first link
        for action in &rule.links[0].actions {
            match action {
                crate::waf::rule::Action::SetVar(set_var) => {
                    if set_var.target.to_ascii_uppercase().starts_with("TX.") {
                        let name = &set_var.target[3..];
                        trx.set_var(name, set_var.value.clone());
                    }
                }
                _ => {}
            }
        }
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
            let transformed = crate::waf::transform::apply_chain(
                &self.ruleset.transform_chains[link.transforms],
                extracted,
            );

            // Evaluate operator
            if self.evaluate_operator(&link.operator, link.rx.as_ref(), transformed) {
                matched = !target.exclude;
                break;
            } else if target.exclude {
            }
        }
        matched
    }

    fn evaluate_operator(
        &self,
        operator: &Operator,
        rx: Option<&Rx>,
        value: Cow<'_, [u8]>,
    ) -> bool {
        // Need to provide macro resolver, captures, data_dir
        // For now, this is a placeholder or I need to pass down more context.
        crate::waf::operator::evaluate_operator(
            operator,
            rx,
            value.as_ref(),
            |_| None,
            &mut [None, None, None, None, None, None, None, None, None, None],
            None,
        )
    }
}
