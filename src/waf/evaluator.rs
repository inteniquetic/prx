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

/// Audit-only hook invocations, per phase (index 0 is phase 1). Phases 4 and 5
/// never block, so this is how a test proves they ran and what they matched.
pub mod audit {
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Phase 4 (response body) evaluations that produced a non-Allow decision.
    pub static PHASE4_MATCHES: AtomicU64 = AtomicU64::new(0);
    /// Phase 5 (logging) evaluations, allowed or not.
    pub static PHASE5_RUNS: AtomicU64 = AtomicU64::new(0);

    pub fn phase4_matches() -> u64 {
        PHASE4_MATCHES.load(Ordering::SeqCst)
    }

    pub fn phase5_runs() -> u64 {
        PHASE5_RUNS.load(Ordering::SeqCst)
    }
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
        let rules = &self.ruleset.phases[phase_index];
        let mut index = 0;
        while let Some(rule) = rules.get(index) {
            index += 1;
            if !self.evaluate_rule(rule, trx) {
                continue;
            }
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
            // Both jumps fire only when the rule matched.
            if let Some(target) = rule.skip_after {
                index = index.max(target);
            }
            for action in &rule.links[0].actions {
                if let crate::waf::rule::Action::Skip(n) = action {
                    index += *n as usize;
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
                })
                .into_owned();
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
        let capture = link
            .actions
            .iter()
            .any(|a| matches!(a, crate::waf::rule::Action::Capture));
        if link.targets.is_empty() {
            return self.evaluate_operator(
                &link.operator,
                link.rx.as_ref(),
                Cow::Borrowed(b""),
                trx,
                capture,
            );
        }
        // Exclusions (`!ARGS:id`) remove values from the same collection.
        let excluded = |collection: &str, name: &str| {
            link.targets.iter().any(|t| {
                t.exclude
                    && t.collection.eq_ignore_ascii_case(collection)
                    && match &t.key {
                        crate::waf::rule::Key::Name(n) => n.eq_ignore_ascii_case(name),
                        crate::waf::rule::Key::Regex(rx) => rx.regex.is_match(name.as_bytes()),
                        crate::waf::rule::Key::All => true,
                    }
            })
        };
        for target in link.targets.iter().filter(|t| !t.exclude) {
            let found = crate::waf::vars::values_cached(trx, target);
            let kept = || {
                found
                    .iter()
                    .filter(|(name, _)| !excluded(&target.collection, name))
            };
            if target.count {
                // `&ARGS` is how many there are, and is evaluated even at zero.
                let count = Cow::Owned(kept().count().to_string().into_bytes());
                if self.evaluate_operator(&link.operator, link.rx.as_ref(), count, trx, capture) {
                    return true;
                }
                continue;
            }
            for (_, value) in kept() {
                let transformed = crate::waf::transform::apply_chain(
                    &self.ruleset.transform_chains[link.transforms],
                    Cow::Borrowed(value.as_ref()),
                );
                if self.evaluate_operator(
                    &link.operator,
                    link.rx.as_ref(),
                    transformed,
                    trx,
                    capture,
                ) {
                    return true;
                }
            }
        }
        false
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
        capture: bool,
    ) -> bool {
        let mut captures: [Option<String>; 10] = Default::default();
        let matched = crate::waf::operator::evaluate_operator(
            operator,
            rx,
            value.as_ref(),
            |key| Self::resolve_macro(trx, key),
            &mut captures,
            None,
        );
        if matched && capture {
            for (i, group) in captures.into_iter().enumerate() {
                match group {
                    Some(text) => trx.set_var(&i.to_string(), text),
                    None => {
                        trx.tx.borrow_mut().remove(&i.to_string());
                    }
                }
            }
        }
        matched
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

    /// Decision plus the TX variables left behind, keyed upper-case.
    fn run(conf: &str, uri: &[u8]) -> (WafDecision, std::collections::HashMap<String, String>) {
        let loaded = crate::waf::load_text(&[("t", conf)]);
        assert_eq!(loaded.summary.skipped, 0, "{loaded}");
        let evaluator = Evaluator::new(Arc::new(loaded.ruleset));
        let head = RequestHeader::build("GET", uri, None).unwrap();
        let trx = Transaction::new(&head, b"");
        let decision = evaluator.evaluate_phase(0, &trx);
        let tx = trx.tx.borrow().clone();
        (decision, tx)
    }

    #[test]
    fn capture_reaches_the_next_rule_as_tx_1() {
        let (decision, tx) = run(
            r#"
SecRule ARGS:q "@rx id=(\d+)" "id:1,phase:1,pass,capture"
SecRule TX:1 "@eq 42" "id:2,phase:1,deny,status:418"
"#,
            b"/x?q=id=42",
        );
        assert_eq!(decision, WafDecision::Deny(418));
        assert_eq!(tx.get("1").map(String::as_str), Some("42"));
    }

    #[test]
    fn skip_after_jumps_only_on_match_and_chain_needs_every_link() {
        let conf = r#"
SecRule ARGS:q "@contains safe" "id:1,phase:1,pass,skipAfter:END"
SecRule ARGS:q "@contains x" "id:2,phase:1,deny,status:403"
SecMarker END
SecRule ARGS:q "@contains a" "id:3,phase:1,pass,chain,setvar:tx.hit=1"
  SecRule ARGS:q "@contains b" "chain"
  SecRule ARGS:q "@contains c" ""
"#;
        assert_eq!(run(conf, b"/?q=safe-x").0, WafDecision::Allow);
        assert_eq!(run(conf, b"/?q=x").0, WafDecision::Deny(403));
        assert_eq!(
            run(conf, b"/?q=abc").1.get("HIT").map(String::as_str),
            Some("1")
        );
        assert_eq!(run(conf, b"/?q=ab").1.get("HIT"), None);
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
