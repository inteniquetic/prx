use crate::waf::rule::{Disruptive, Operator, Rule, RuleEngine, RuleLink, RuleSet, Rx};
use crate::waf::vars::Transaction;
use std::borrow::Cow;
use std::sync::Arc;

/// Enforcement result, separate from whether a non-disruptive rule matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WafDecision {
    Allow,
    Deny(u16),
    Drop,
    Redirect(String),
}

pub struct Evaluator {
    ruleset: Arc<RuleSet>,
}

impl Evaluator {
    pub fn new(ruleset: Arc<RuleSet>) -> Self {
        Self { ruleset }
    }

    pub fn evaluate_request_head<'a>(&'a self, trx: &'a Transaction<'a>) -> WafDecision {
        self.evaluate_phase(0, trx)
    }

    /// Evaluate a zero-based phase with the same transaction across phases.
    pub fn evaluate_phase<'a>(
        &'a self,
        phase_index: usize,
        trx: &'a Transaction<'a>,
    ) -> WafDecision {
        if self.ruleset.settings.rule_engine == Some(RuleEngine::Off) {
            return WafDecision::Allow;
        }
        if phase_index == 0 {
            // Before any rule reads them, so CRS's own defaults (`&TX:... @eq 0`)
            // see the configured values and leave them alone.
            let tuning = &self.ruleset.tuning;
            trx.set_var("blocking_paranoia_level", tuning.paranoia_level.to_string());
            trx.set_var(
                "inbound_anomaly_score_threshold",
                tuning.inbound_anomaly_threshold.to_string(),
            );
        }
        let detection = self.ruleset.settings.rule_engine == Some(RuleEngine::DetectionOnly);
        for rule in &self.ruleset.phases[phase_index] {
            if self.evaluate_rule(rule, trx) {
                // Rule matched, trigger actions
                self.process_actions(rule, trx);
                let decision = match &rule.disruptive {
                    Disruptive::Deny => WafDecision::Deny(rule.status.unwrap_or(403)),
                    Disruptive::Drop => WafDecision::Drop,
                    Disruptive::Redirect(url) => WafDecision::Redirect(url.clone()),
                    _ => WafDecision::Allow,
                };
                if !detection && decision != WafDecision::Allow {
                    return decision;
                }
            }
        }
        WafDecision::Allow
    }

    fn process_actions(&self, rule: &Rule, trx: &Transaction) {
        // Actions are on the first link
        for action in &rule.links[0].actions {
            if let crate::waf::rule::Action::SetVar(set_var) = action
                && set_var.target.to_ascii_uppercase().starts_with("TX.")
            {
                let name = &set_var.target[3..];
                use crate::waf::rule::SetVarOp;
                let value = crate::waf::operator::expand_macros(&set_var.value, &|key| {
                    Self::resolve_macro(trx, key)
                });
                match set_var.op {
                    SetVarOp::Set => trx.set_var(name, value),
                    SetVarOp::Add | SetVarOp::Sub => {
                        let current = trx
                            .get_var(name)
                            .and_then(|v| v.parse::<i64>().ok())
                            .unwrap_or(0);
                        let delta = value.parse::<i64>().unwrap_or(0);
                        let next = if set_var.op == SetVarOp::Add {
                            current.saturating_add(delta)
                        } else {
                            current.saturating_sub(delta)
                        };
                        trx.set_var(name, next.to_string());
                    }
                    SetVarOp::Unset => {
                        trx.tx.borrow_mut().remove(&name.to_ascii_uppercase());
                    }
                }
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
        if link.targets.is_empty() {
            return self.evaluate_operator(
                &link.operator,
                link.rx.as_ref(),
                Cow::Borrowed(b""),
                trx,
            );
        }
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
            if self.evaluate_operator(&link.operator, link.rx.as_ref(), transformed, trx) {
                matched = !target.exclude;
                break;
            } else if target.exclude {
            }
        }
        matched
    }

    fn resolve_macro(trx: &Transaction, key: &str) -> Option<String> {
        let (collection, name) = key.split_once('.')?;
        collection
            .eq_ignore_ascii_case("tx")
            .then(|| trx.get_var(name))
            .flatten()
    }

    fn evaluate_operator(
        &self,
        operator: &Operator,
        rx: Option<&Rx>,
        value: Cow<'_, [u8]>,
        trx: &Transaction,
    ) -> bool {
        crate::waf::operator::evaluate_operator(
            operator,
            rx,
            value.as_ref(),
            |key| Self::resolve_macro(trx, key),
            &mut [None, None, None, None, None, None, None, None, None, None],
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pingora::http::RequestHeader;

    #[test]
    fn score_accumulates_until_final_phase_default_action() {
        let loaded = crate::waf::load_text(&[(
            "test",
            r#"
SecDefaultAction "phase:1,pass"
SecDefaultAction "phase:2,deny,status:406"
SecAction "id:1,phase:1,pass,setvar:tx.score=0,setvar:tx.threshold=5,setvar:tx.critical=3"
SecAction "id:2,phase:1,pass,setvar:tx.score=+%{tx.critical}"
SecAction "id:3,phase:2,pass,setvar:tx.score=+%{tx.critical}"
SecRule TX:score "@ge %{tx.threshold}" "id:4,phase:2,block"
"#,
        )]);
        assert_eq!(loaded.summary.rules, 4, "{loaded}");
        let evaluator = Evaluator::new(Arc::new(loaded.ruleset));
        let head = RequestHeader::build("GET", b"/", None).unwrap();
        let trx = Transaction::new(&head, b"");
        assert_eq!(evaluator.evaluate_request_head(&trx), WafDecision::Allow);
        assert_eq!(trx.get_var("score").as_deref(), Some("3"));
        assert_eq!(evaluator.evaluate_phase(1, &trx), WafDecision::Deny(406));
        assert_eq!(trx.get_var("score").as_deref(), Some("6"));
    }

    #[test]
    fn setvar_only_updates_tx_and_accepts_mixed_case_prefixes() {
        let loaded = crate::waf::load_text(&[(
            "test.conf",
            r#"SecAction "id:1,phase:1,pass,nolog,setvar:TX.upper=one,setvar:tx.lower=two,setvar:Tx.mixed=three,setvar:IP.other=ignored""#,
        )]);
        assert_eq!(loaded.summary.rules, 1, "{loaded}");
        let evaluator = Evaluator::new(Arc::new(loaded.ruleset));
        let head = RequestHeader::build("GET", b"/", None).unwrap();
        let trx = Transaction::new(&head, b"");
        evaluator.process_actions(&evaluator.ruleset.phases[0][0], &trx);
        assert_eq!(trx.get_var("upper").as_deref(), Some("one"));
        assert_eq!(trx.get_var("lower").as_deref(), Some("two"));
        assert_eq!(trx.get_var("mixed").as_deref(), Some("three"));
        assert_eq!(trx.tx.borrow().len(), 3);
    }
}
