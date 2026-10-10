//! WAF rules: SecLang files in, a compiled ruleset out (T505).
//!
//! [`load`] is the whole API. It reads rules files, parses them
//! ([`parse`]), and resolves everything that can be resolved before a request
//! exists ([`rule::RuleSet`]): default actions, chains, removals and target
//! updates, `skipAfter` jumps, compiled patterns, the transformation chains
//! and the set of collections the rules read. Evaluating the result is T506.
//!
//! A load never fails. A rule that cannot be parsed or compiled is left out
//! and reported with its file, line and id; whoever calls [`load`] decides
//! whether the [`Loaded::issues`] are acceptable, and should log the
//! [`Loaded`] (it implements `Display`) either way.

pub mod parse;
mod pcre;
pub mod rule;
pub mod transform;
pub mod operator;
pub mod evaluator;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fmt, fs};

use serde::Serialize;

use crate::config::IssueSeverity;
use rule::{
    Action, Directive, DirectiveKind, Disruptive, Key, Link, Operator, Rule, RuleLink, RuleSet,
    Selector, Target, Variable,
};

/// One problem in a rules file, addressed the way [`crate::validate::Diagnostic`]
/// addresses a config problem so the Web UI (T509) can mark the line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    /// An error means something was left out of the ruleset; a warning means
    /// it was loaded but probably does not do what its author meant.
    pub severity: IssueSeverity,
    /// Stable identifier, so the UI can translate the message.
    pub code: &'static str,
    pub file: String,
    /// 1-based; 0 when the problem is the file itself.
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<u64>,
    pub message: String,
}

impl Issue {
    pub(crate) fn error(
        code: &'static str,
        file: &str,
        line: u32,
        rule_id: Option<u64>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: IssueSeverity::Error,
            code,
            file: file.to_string(),
            line,
            rule_id,
            message: message.into(),
        }
    }

    fn warning(
        code: &'static str,
        file: &str,
        line: u32,
        rule_id: Option<u64>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: IssueSeverity::Warning,
            ..Self::error(code, file, line, rule_id, message)
        }
    }
}

/// What a load amounted to, in numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub files: usize,
    /// Every directive line seen, by name, whether or not it was usable.
    pub directives: BTreeMap<String, usize>,
    /// Rules in the ruleset. A chain is one rule.
    pub rules: usize,
    /// Rules left out because of an error; [`Loaded::issues`] says why.
    pub skipped: usize,
    /// Rules taken out by `SecRuleRemoveById` / `SecRuleRemoveByTag`.
    pub removed: usize,
    /// `@rx` patterns compiled, and how many of them needed the PCRE repair.
    pub rx: usize,
    pub rx_repaired: usize,
}

/// The result of [`load`]: always a usable ruleset, plus everything that went
/// wrong on the way to it.
#[derive(Debug, Default)]
pub struct Loaded {
    pub ruleset: RuleSet,
    pub issues: Vec<Issue>,
    pub summary: Summary,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            IssueSeverity::Error => "error",
            IssueSeverity::Warning => "warning",
        };
        write!(f, "{severity} {}:{}", self.file, self.line)?;
        if let Some(id) = self.rule_id {
            write!(f, " [rule {id}]")?;
        }
        write!(f, " {}", self.message)
    }
}

/// The summary line, then one line per issue. Meant for the log: a rule that
/// silently did not load is a hole in the WAF nobody knows about.
impl fmt::Display for Loaded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let summary = &self.summary;
        let by_name: Vec<String> = summary
            .directives
            .iter()
            .map(|(name, count)| format!("{name} {count}"))
            .collect();
        let errors = self
            .issues
            .iter()
            .filter(|issue| issue.severity == IssueSeverity::Error)
            .count();
        write!(
            f,
            "waf: {} files, {} directives ({}); {} rules loaded, {} skipped, {} removed; \
             {} @rx compiled, {} of them repaired; {} errors, {} warnings",
            summary.files,
            summary.directives.values().sum::<usize>(),
            by_name.join(", "),
            summary.rules,
            summary.skipped,
            summary.removed,
            summary.rx,
            summary.rx_repaired,
            errors,
            self.issues.len() - errors,
        )?;
        for issue in &self.issues {
            write!(f, "\n  {issue}")?;
        }
        Ok(())
    }
}

/// Loads rules files, in the order given. A file name may hold `*` and `?`
/// (`rules/*.conf`); matches load in sorted order, as CRS's numbering expects.
///
/// Pure apart from reading the files: the caller owns the swap and the log.
pub fn load(paths: &[PathBuf]) -> Loaded {
    let mut builder = Builder::default();
    for pattern in paths {
        for path in builder.expand(pattern) {
            let name = path.display().to_string();
            match fs::read_to_string(&path) {
                Ok(text) => builder.file(&name, &text),
                Err(err) => {
                    let issue = Issue::error("unreadable_file", &name, 0, None, err.to_string());
                    builder.loaded.issues.push(issue);
                }
            }
        }
    }
    builder.finish()
}

/// [`load`] for text that is already in memory, as `(file name, text)`.
pub fn load_text(sources: &[(&str, &str)]) -> Loaded {
    let mut builder = Builder::default();
    for (name, text) in sources {
        builder.file(name, text);
    }
    builder.finish()
}

/// What a rule inherits for its phase, and — once its own actions are layered
/// on top — what it ends up with.
#[derive(Debug, Clone)]
struct Defaults {
    disruptive: Disruptive,
    status: Option<u16>,
    log: bool,
    audit_log: bool,
    transforms: Vec<String>,
}

/// ModSecurity's built-in `SecDefaultAction "phase:2,log,auditlog,pass"`.
impl Default for Defaults {
    fn default() -> Self {
        Self {
            disruptive: Disruptive::Pass,
            status: None,
            log: true,
            audit_log: true,
            transforms: Vec::new(),
        }
    }
}

impl Defaults {
    fn apply(&mut self, actions: &[Action]) {
        let inherited = self.disruptive.clone();
        for action in actions {
            match action {
                // `block` is "whatever the default says", even after a `deny`.
                Action::Disruptive(Disruptive::Block) => self.disruptive = inherited.clone(),
                Action::Disruptive(disruptive) => self.disruptive = disruptive.clone(),
                Action::Status(code) => self.status = Some(*code),
                Action::Log => (self.log, self.audit_log) = (true, true),
                Action::NoLog => (self.log, self.audit_log) = (false, false),
                Action::AuditLog => self.audit_log = true,
                Action::NoAuditLog => self.audit_log = false,
                Action::Transform(name) if name.eq_ignore_ascii_case("none") => {
                    self.transforms.clear();
                }
                Action::Transform(name) => self.transforms.push(name.to_ascii_lowercase()),
                _ => {}
            }
        }
    }
}

fn find<T>(actions: &[Action], pick: impl Fn(&Action) -> Option<T>) -> Option<T> {
    actions.iter().find_map(pick)
}

fn phase_of(actions: &[Action]) -> u8 {
    find(actions, |action| match action {
        Action::Phase(phase) => Some(*phase),
        _ => None,
    })
    .unwrap_or(2)
}

/// Rules and markers in file order. They stay interleaved until the end of the
/// load because a removal shifts every index after it.
enum Item {
    Rule(Rule),
    Marker(String),
}

#[derive(Default)]
struct Builder {
    loaded: Loaded,
    items: Vec<Item>,
    /// Per phase (index 0 is phase 1), as ModSecurity v3 and Coraza keep them.
    defaults: [Defaults; 5],
    ids: HashSet<u64>,
    chains: HashMap<Vec<String>, usize>,
}

impl Builder {
    /// ponytail: wildcards in the file name only, which is all `rules/*.conf`
    /// asks for. A `*` in a directory name is the day to take the glob crate.
    fn expand(&mut self, pattern: &Path) -> Vec<PathBuf> {
        let name = pattern.file_name().and_then(|name| name.to_str());
        let Some(name) = name.filter(|name| name.contains(['*', '?'])) else {
            return vec![pattern.to_path_buf()];
        };
        let wildcard = regex::escape(name).replace(r"\*", ".*").replace(r"\?", ".");
        let matcher = regex::Regex::new(&format!("^{wildcard}$")).ok();
        let dir = match pattern.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        };
        let mut found: Vec<PathBuf> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path.file_name().and_then(|name| name.to_str());
                path.is_file()
                    && name.is_some_and(|name| matcher.as_ref().is_some_and(|m| m.is_match(name)))
            })
            .collect();
        found.sort();
        if found.is_empty() {
            let file = pattern.display().to_string();
            let issue = Issue::error("no_files", &file, 0, None, "no file matches this pattern");
            self.loaded.issues.push(issue);
        }
        found
    }

    fn file(&mut self, name: &str, text: &str) {
        let parsed = parse::parse(name, text);
        let summary = &mut self.loaded.summary;
        summary.files += 1;
        summary.skipped += parsed.skipped;
        for (directive, count) in parsed.counts {
            *summary.directives.entry(directive).or_default() += count;
        }
        let first = self.loaded.issues.len();
        self.loaded.issues.extend(parsed.issues);

        let file: Arc<str> = Arc::from(name);
        for Directive { line, kind } in parsed.directives {
            self.directive(&file, line, kind);
        }
        // Parsing and compiling each report in their own pass; a reader wants
        // one list, top of the file to the bottom.
        self.loaded.issues[first..].sort_by_key(|issue| issue.line);
    }

    fn directive(&mut self, file: &Arc<str>, line: u32, kind: DirectiveKind) {
        match kind {
            DirectiveKind::Rule(links) => self.rule(file, line, links),
            DirectiveKind::Action(actions) => {
                let link = Link {
                    line,
                    variables: Vec::new(),
                    operator: Operator {
                        name: "unconditionalMatch".into(),
                        negated: false,
                        argument: String::new(),
                    },
                    actions,
                };
                self.rule(file, line, vec![link]);
            }
            DirectiveKind::DefaultAction(actions) => {
                let mut defaults = Defaults::default();
                defaults.apply(&actions);
                self.defaults[usize::from(phase_of(&actions)) - 1] = defaults;
            }
            DirectiveKind::Marker(name) => self.items.push(Item::Marker(name)),
            DirectiveKind::ComponentSignature(text) => {
                self.loaded.ruleset.component_signatures.push(text);
            }
            DirectiveKind::RemoveById(ids) => {
                self.remove(file, line, |rule| {
                    ids.iter().any(|range| range.contains(rule.id))
                });
            }
            DirectiveKind::RemoveByTag(tag) => {
                self.remove(file, line, |rule| rule.tags().any(|have| have == tag));
            }
            DirectiveKind::UpdateTargetById(ids, variables) => {
                let targets: Result<Vec<Target>, String> =
                    variables.into_iter().map(target).collect();
                let targets = match targets {
                    Ok(targets) => targets,
                    Err(message) => {
                        let issue = Issue::error("invalid_regex", file, line, None, message);
                        return self.loaded.issues.push(issue);
                    }
                };
                let mut matched = false;
                for item in &mut self.items {
                    if let Item::Rule(rule) = item
                        && ids.iter().any(|range| range.contains(rule.id))
                    {
                        // The head only, as ModSecurity does: widening an
                        // exclusion to links it was not written for would
                        // switch off more than was asked.
                        rule.links[0].targets.extend(targets.iter().cloned());
                        matched = true;
                    }
                }
                if !matched {
                    self.no_match(file, line);
                }
            }
            DirectiveKind::CollectionTimeout(seconds) => {
                self.loaded.ruleset.collection_timeout = Some(seconds);
            }
            DirectiveKind::Setting(setting) => self.loaded.ruleset.settings.apply(setting),
        }
    }

    /// Removals and updates reach the rules loaded before them, as they do in
    /// ModSecurity v2 and Coraza. Reaching forward as well would let a
    /// misplaced exclusion switch off a rule that used to run; this way it
    /// does nothing, and says so.
    fn no_match(&mut self, file: &str, line: u32) {
        let message = "matches no rule loaded before it; it has no effect";
        let issue = Issue::warning("no_match", file, line, None, message);
        self.loaded.issues.push(issue);
    }

    fn remove(&mut self, file: &str, line: u32, matches: impl Fn(&Rule) -> bool) {
        let before = self.items.len();
        self.items.retain(|item| match item {
            Item::Rule(rule) if matches(rule) => {
                self.ids.remove(&rule.id);
                false
            }
            _ => true,
        });
        let removed = before - self.items.len();
        self.loaded.summary.removed += removed;
        if removed == 0 {
            self.no_match(file, line);
        }
    }

    fn rule(&mut self, file: &Arc<str>, line: u32, links: Vec<Link>) {
        let head = &links[0].actions;
        let id = find(head, |action| match action {
            Action::Id(id) => Some(*id),
            _ => None,
        });
        let problem = match id {
            None => Some(("missing_id", "a rule needs an `id` action")),
            Some(id) if self.ids.contains(&id) => Some((
                "duplicate_id",
                "this id is already taken; the first rule keeps it",
            )),
            Some(_) => None,
        };
        if let Some((code, message)) = problem {
            self.loaded.summary.skipped += 1;
            let issue = Issue::error(code, file, line, id, message);
            return self.loaded.issues.push(issue);
        }
        let id = id.unwrap_or_default();

        let phase = phase_of(head);
        let defaults = &self.defaults[usize::from(phase) - 1];
        let mut resolved = defaults.clone();
        resolved.apply(head);

        let chained = links.len() > 1;
        let mut compiled = Vec::with_capacity(links.len());
        for link in links {
            let mut own = defaults.clone();
            own.apply(&link.actions);
            let next = self.chains.len();
            let transforms = *self.chains.entry(own.transforms).or_insert(next);

            let link_line = link.line;
            match compile_link(link, transforms) {
                Ok((link, warning)) => {
                    let warning = warning
                        .map(|text| Issue::warning("rx_macro", file, link_line, Some(id), text));
                    self.loaded.issues.extend(warning);
                    compiled.push(link);
                }
                Err(mut message) => {
                    if chained {
                        message.push_str("; the whole chain is skipped");
                    }
                    self.loaded.summary.skipped += 1;
                    let issue = Issue::error("invalid_regex", file, link_line, Some(id), message);
                    return self.loaded.issues.push(issue);
                }
            }
        }

        self.ids.insert(id);
        self.items.push(Item::Rule(Rule {
            file: Arc::clone(file),
            line,
            id,
            phase,
            disruptive: resolved.disruptive,
            status: resolved.status,
            log: resolved.log,
            audit_log: resolved.audit_log,
            skip_after: None,
            links: compiled,
        }));
    }

    fn finish(mut self) -> Loaded {
        let phases = &mut self.loaded.ruleset.phases;
        // (phase index, rule index, marker) of every jump still looking for
        // its marker. A marker only ends the jumps that come before it.
        let mut pending: Vec<(usize, usize, String)> = Vec::new();
        for item in self.items {
            match item {
                Item::Rule(rule) => {
                    let phase = usize::from(rule.phase) - 1;
                    let marker = find(&rule.links[0].actions, |action| match action {
                        Action::SkipAfter(marker) => Some(marker.clone()),
                        _ => None,
                    });
                    pending.extend(marker.map(|marker| (phase, phases[phase].len(), marker)));
                    phases[phase].push(rule);
                }
                Item::Marker(name) => pending.retain(|(phase, rule, marker)| {
                    if *marker == name {
                        phases[*phase][*rule].skip_after = Some(phases[*phase].len());
                    }
                    *marker != name
                }),
            }
        }
        // ModSecurity skips the rest of the phase when the marker never comes.
        for (phase, rule, marker) in pending {
            let end = phases[phase].len();
            let rule = &mut phases[phase][rule];
            rule.skip_after = Some(end);
            let message = format!(
                "skipAfter:{marker} has no SecMarker after it; it skips the rest of phase {}",
                rule.phase
            );
            let (file, line, id) = (&rule.file, rule.line, Some(rule.id));
            let issue = Issue::warning("unresolved_marker", file, line, id, message);
            self.loaded.issues.push(issue);
        }

        let mut chains: Vec<_> = self.chains.into_iter().collect();
        chains.sort_by_key(|(_, index)| *index);
        self.loaded.ruleset.transform_chains = chains.into_iter().map(|(chain, _)| chain).collect();

        let ruleset = &mut self.loaded.ruleset;
        let summary = &mut self.loaded.summary;
        for link in ruleset.phases.iter().flatten().flat_map(|rule| &rule.links) {
            summary.rx += usize::from(link.rx.is_some());
            summary.rx_repaired += usize::from(link.rx.as_ref().is_some_and(|rx| rx.repaired));

            let targets = link.targets.iter().filter(|target| !target.exclude);
            ruleset
                .needs
                .extend(targets.map(|target| target.collection.clone()));
            macro_collections(&link.operator.argument, &mut ruleset.needs);
            for action in &link.actions {
                macro_collections(&action.to_string(), &mut ruleset.needs);
            }
        }
        summary.rules = ruleset.phases.iter().map(Vec::len).sum();
        self.loaded
    }
}

/// The collection named by every `%{collection.key}` / `%{variable}` in `text`.
fn macro_collections(text: &str, needs: &mut BTreeSet<String>) {
    for after in text.split("%{").skip(1) {
        let name = after
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()
            .unwrap_or_default();
        if !name.is_empty() {
            needs.insert(name.to_ascii_uppercase());
        }
    }
}

fn target(variable: Variable) -> Result<Target, String> {
    let key = match variable.selector {
        None => Key::All,
        Some(Selector::Name(name)) => Key::Name(name),
        // Entry names are matched case-insensitively, as ModSecurity does.
        Some(Selector::Regex(pattern)) => Key::Regex(
            pcre::compile(&pattern, true)
                .map_err(|reason| format!("selector /{pattern}/ does not compile: {reason}"))?,
        ),
    };
    Ok(Target {
        collection: variable.collection,
        key,
        exclude: variable.exclude,
        count: variable.count,
    })
}

/// The link, and a warning when its `@rx` has to wait for the request.
fn compile_link(link: Link, transforms: usize) -> Result<(RuleLink, Option<String>), String> {
    let targets: Result<Vec<Target>, String> = link.variables.into_iter().map(target).collect();
    let pattern = &link.operator.argument;
    let (rx, warning) = match (link.operator.is_rx(), pattern.contains("%{")) {
        (false, _) => (None, None),
        (true, true) => {
            let message = "the pattern holds a macro, so it cannot be compiled at load";
            (None, Some(message.to_string()))
        }
        (true, false) => {
            let rx = pcre::compile(pattern, false)
                .map_err(|reason| format!("pattern does not compile: {reason}"))?;
            (Some(rx), None)
        }
    };
    let link = RuleLink {
        line: link.line,
        targets: targets?,
        operator: link.operator,
        rx,
        transforms,
        actions: link.actions,
    };
    Ok((link, warning))
}

#[cfg(test)]
mod tests {
    use super::rule::{BodyLimitAction, RuleEngine};
    use super::*;

    fn loaded(text: &str) -> Loaded {
        load_text(&[("test.conf", text)])
    }

    fn clean(text: &str) -> Loaded {
        let loaded = loaded(text);
        assert_eq!(loaded.issues, [], "{text}");
        loaded
    }

    fn ids(loaded: &Loaded, phase: usize) -> Vec<u64> {
        loaded.ruleset.phases[phase - 1]
            .iter()
            .map(|r| r.id)
            .collect()
    }

    // CRS v4.21.0, rule 941380 (Apache-2.0), tags shortened. One of the seven
    // patterns T504 found `regex` rejecting as written: to PCRE these braces
    // are literals, to `regex` they are a quantifier with nothing in it.
    const CRS_941380: &str = r#"SecRule REQUEST_COOKIES|REQUEST_COOKIES_NAMES|ARGS_NAMES|ARGS|REQUEST_FILENAME|XML:/* "@rx {{.*?}}" \
    "id:941380,\
    phase:2,\
    block,\
    capture,\
    t:none,\
    msg:'AngularJS client side template injection detected',\
    logdata:'Matched Data: Suspicious payload found within %{MATCHED_VAR_NAME}: %{MATCHED_VAR}',\
    tag:'attack-xss',\
    tag:'paranoia-level/2',\
    ver:'OWASP_CRS/4.21.0',\
    severity:'CRITICAL',\
    setvar:'tx.xss_score=+%{tx.critical_anomaly_score}',\
    setvar:'tx.inbound_anomaly_score_pl2=+%{tx.critical_anomaly_score}'"
"#;

    // CRS v4.21.0, rule 911100 (Apache-2.0), tags shortened.
    const CRS_911100: &str = r#"SecRule REQUEST_METHOD "!@within %{tx.allowed_methods}" \
    "id:911100,phase:1,block,t:none,\
    msg:'Method is not allowed by policy',\
    logdata:'%{MATCHED_VAR}',\
    severity:'CRITICAL',\
    setvar:'tx.inbound_anomaly_score_pl1=+%{tx.critical_anomaly_score}'"
"#;

    /// The ruleset is shared by every worker thread behind an `Arc` and
    /// replaced by swapping it, the way `RuntimeConfig` is.
    #[test]
    fn a_ruleset_can_be_shared_across_threads() {
        fn shared<T: Send + Sync + 'static>() {}
        shared::<RuleSet>();
    }

    #[test]
    fn rules_land_in_their_phase_in_file_order() {
        let loaded = clean(concat!(
            "SecRule ARGS \"@rx a\" \"id:1,phase:2,pass\"\n",
            "SecAction \"id:2,phase:1,pass,nolog\"\n",
            "SecRule ARGS \"@rx a\" \"id:3,pass\"\n",
            "SecRule RESPONSE_BODY \"@rx a\" \"id:4,phase:response,pass\"\n",
        ));
        assert_eq!(ids(&loaded, 1), [2]);
        assert_eq!(ids(&loaded, 2), [1, 3], "no phase means phase 2");
        assert_eq!(ids(&loaded, 4), [4]);
        let action = &loaded.ruleset.phases[0][0];
        assert_eq!(action.links[0].operator.name, "unconditionalMatch");
        assert!(action.links[0].targets.is_empty());
        assert_eq!(&*action.file, "test.conf");
        assert_eq!(loaded.summary.rules, 4);
    }

    #[test]
    fn default_actions_are_inherited_per_phase() {
        let loaded = clean(concat!(
            "SecRule ARGS \"@rx a\" \"id:1,phase:1,block\"\n",
            "SecDefaultAction \"phase:1,nolog,auditlog,deny,status:403,t:lowercase\"\n",
            "SecDefaultAction \"phase:2,log,noauditlog,pass\"\n",
            "SecRule ARGS \"@rx a\" \"id:2,phase:1,block,t:trim\"\n",
            "SecRule ARGS \"@rx a\" \"id:3,phase:1,t:none,t:trim,drop,status:500,log\"\n",
            "SecRule ARGS \"@rx a\" \"id:4,phase:1\"\n",
            "SecRule ARGS \"@rx a\" \"id:5,phase:2,block\"\n",
        ));
        let rule = |id: u64| {
            let all = loaded.ruleset.phases.iter().flatten();
            all.clone().find(|r| r.id == id).unwrap()
        };
        let chain = |id: u64| &loaded.ruleset.transform_chains[rule(id).links[0].transforms];

        // Before any SecDefaultAction: ModSecurity's own default.
        assert_eq!(rule(1).disruptive, Disruptive::Pass);
        assert!(rule(1).log && rule(1).audit_log);

        assert_eq!(rule(2).disruptive, Disruptive::Deny);
        assert_eq!(rule(2).status, Some(403));
        assert!(!rule(2).log && rule(2).audit_log);
        assert_eq!(chain(2), &["lowercase", "trim"]);

        assert_eq!(rule(3).disruptive, Disruptive::Drop);
        assert_eq!(rule(3).status, Some(500));
        assert!(rule(3).log);
        assert_eq!(chain(3), &["trim"], "t:none drops what was inherited");

        assert_eq!(
            rule(4).disruptive,
            Disruptive::Deny,
            "no action at all inherits too"
        );
        assert_eq!(rule(5).disruptive, Disruptive::Pass);
        assert!(rule(5).log && !rule(5).audit_log);
    }

    #[test]
    fn equal_transformation_chains_share_one_key() {
        let loaded = clean(concat!(
            "SecRule ARGS \"@rx a\" \"id:1,pass,t:none,t:urlDecodeUni,t:lowercase\"\n",
            "SecRule ARGS \"@rx a\" \"id:2,pass,t:URLDECODEUNI,t:lowercase,chain\"\n",
            "  SecRule ARGS \"@rx a\" \"t:lowercase,t:urlDecodeUni\"\n",
            "SecRule ARGS \"@rx a\" \"id:3,pass\"\n",
        ));
        let rules = &loaded.ruleset.phases[1];
        assert_eq!(rules[0].links[0].transforms, rules[1].links[0].transforms);
        assert_ne!(
            rules[1].links[0].transforms, rules[1].links[1].transforms,
            "order matters"
        );
        assert_eq!(loaded.ruleset.transform_chains.len(), 3);
        let none = rules[2].links[0].transforms;
        assert!(loaded.ruleset.transform_chains[none].is_empty());
    }

    #[test]
    fn removals_apply_to_the_rules_before_them() {
        let loaded = loaded(concat!(
            "SecRule ARGS \"@rx a\" \"id:1,pass,tag:'attack-sqli'\"\n",
            "SecRule ARGS \"@rx a\" \"id:2,pass,tag:'attack-xss'\"\n",
            "SecRule ARGS \"@rx a\" \"id:10,pass\"\n",
            "SecRule ARGS \"@rx a\" \"id:11,pass\"\n",
            "SecRule ARGS \"@rx a\" \"id:12,pass\"\n",
            "SecRuleRemoveById 2 10-11 99\n",
            "SecRuleRemoveByTag \"attack-sqli\"\n",
            "SecRuleRemoveByTag \"attack-rce\"\n",
            "SecRuleRemoveById 13\n",
            "SecRule ARGS \"@rx a\" \"id:13,pass\"\n",
        ));
        assert_eq!(ids(&loaded, 2), [12, 13]);
        assert_eq!(loaded.summary.removed, 4);
        let seen: Vec<_> = loaded
            .issues
            .iter()
            .map(|i| (i.line, i.code, i.severity))
            .collect();
        assert_eq!(
            seen,
            [
                (8, "no_match", IssueSeverity::Warning),
                (9, "no_match", IssueSeverity::Warning)
            ],
            "a removal that removes nothing is worth saying; 99 rode along with ids that matched"
        );
    }

    #[test]
    fn a_removed_id_can_be_defined_again() {
        let loaded = clean(concat!(
            "SecRule ARGS \"@rx a\" \"id:1,pass\"\n",
            "SecRuleRemoveById 1\n",
            "SecRule ARGS \"@rx b\" \"id:1,pass\"\n",
        ));
        assert_eq!(loaded.ruleset.phases[1][0].links[0].operator.argument, "b");
    }

    #[test]
    fn target_updates_extend_the_rule_they_name() {
        let loaded = loaded(concat!(
            "SecRule ARGS|REQUEST_COOKIES \"@rx a\" \"id:1,pass,chain\"\n",
            "  SecRule TX:x \"@eq 1\"\n",
            "SecRuleUpdateTargetById 1 \"!ARGS:foo|!REQUEST_COOKIES:/^sess/\"\n",
            "SecRuleUpdateTargetById 2 \"!ARGS:foo\"\n",
            "SecRuleUpdateTargetById 1 \"!ARGS:/(/\"\n",
        ));
        let rule = &loaded.ruleset.phases[1][0];
        let targets = &rule.links[0].targets;
        assert_eq!(targets.len(), 4);
        assert!(targets[2].exclude && matches!(&targets[2].key, Key::Name(n) if n == "foo"));
        assert!(matches!(&targets[3].key, Key::Regex(rx) if rx.regex.is_match(b"SESSID")));
        assert_eq!(
            rule.links[1].targets.len(),
            1,
            "only the head of a chain is updated"
        );
        let seen: Vec<_> = loaded.issues.iter().map(|i| (i.line, i.code)).collect();
        assert_eq!(seen, [(4, "no_match"), (5, "invalid_regex")]);
    }

    #[test]
    fn skip_after_is_resolved_to_an_index_in_its_own_phase() {
        let loaded = loaded(concat!(
            "SecRule ARGS \"@rx a\" \"id:1,phase:1,pass,skipAfter:END\"\n",
            "SecRule ARGS \"@rx a\" \"id:2,phase:2,pass,skipAfter:END\"\n",
            "SecRule ARGS \"@rx a\" \"id:3,phase:1,pass\"\n",
            "SecRule ARGS \"@rx a\" \"id:4,phase:1,pass\"\n",
            "SecRuleRemoveById 4\n",
            "SecMarker END\n",
            "SecRule ARGS \"@rx a\" \"id:5,phase:1,pass,skipAfter:NOWHERE\"\n",
            "SecRule ARGS \"@rx a\" \"id:6,phase:1,pass\"\n",
            "SecMarker END\n",
        ));
        let phase1 = &loaded.ruleset.phases[0];
        assert_eq!(ids(&loaded, 1), [1, 3, 5, 6]);
        assert_eq!(
            phase1[0].skip_after,
            Some(2),
            "lands on rule 5, the first after the marker"
        );
        assert_eq!(
            loaded.ruleset.phases[1][0].skip_after,
            Some(1),
            "one past the end"
        );
        assert_eq!(phase1[1].skip_after, None);
        // ModSecurity skips the rest of the phase when the marker never comes.
        assert_eq!(phase1[2].skip_after, Some(4));
        let [issue] = loaded.issues.as_slice() else {
            panic!("{:?}", loaded.issues)
        };
        assert_eq!(
            (issue.code, issue.severity, issue.line, issue.rule_id),
            ("unresolved_marker", IssueSeverity::Warning, 7, Some(5))
        );
    }

    #[test]
    fn rx_is_compiled_at_load_and_a_repair_is_recorded() {
        let loaded = clean(&format!("{CRS_941380}{CRS_911100}"));
        let repaired = &loaded.ruleset.phases[1][0].links[0];
        let rx = repaired.rx.as_ref().unwrap();
        assert!(rx.repaired);
        assert_eq!(
            repaired.operator.argument, "{{.*?}}",
            "the rule still says what CRS said"
        );
        assert!(
            rx.regex
                .is_match(b"{{constructor.constructor('alert(1)')()}}")
        );
        assert!(!rx.regex.is_match(b"{constructor}"));

        let other = &loaded.ruleset.phases[0][0].links[0];
        assert!(other.rx.is_none(), "@within has nothing to compile");
        assert!(other.operator.negated);
        assert_eq!((loaded.summary.rx, loaded.summary.rx_repaired), (1, 1));

        let normal = clean("SecRule ARGS \"@rx ^[a-z]{1,3}$\" \"id:1,pass\"\n");
        let rx = normal.ruleset.phases[1][0].links[0].rx.as_ref().unwrap();
        assert!(!rx.repaired);
        assert_eq!((normal.summary.rx, normal.summary.rx_repaired), (1, 0));
    }

    #[test]
    fn a_pattern_that_cannot_compile_costs_its_whole_chain_and_nothing_else() {
        let loaded = loaded(concat!(
            "SecRule ARGS \"@rx a\" \"id:1,pass,chain\"\n",
            "  SecRule ARGS \\\n",
            "    \"@rx (?<=a)b\"\n",
            "SecRule ARGS:/(?!x)/ \"@rx a\" \"id:2,pass\"\n",
            "SecRule ARGS \"@rx ok\" \"id:3,pass\"\n",
        ));
        assert_eq!(ids(&loaded, 2), [3]);
        let seen: Vec<_> = loaded
            .issues
            .iter()
            .map(|i| (i.file.as_str(), i.line, i.code, i.rule_id))
            .collect();
        assert_eq!(
            seen,
            [
                ("test.conf", 2, "invalid_regex", Some(1)),
                ("test.conf", 4, "invalid_regex", Some(2))
            ]
        );
        assert!(
            loaded.issues[0].message.contains("look"),
            "{}",
            loaded.issues[0].message
        );
        assert_eq!(loaded.summary.skipped, 2);
    }

    #[test]
    fn an_rx_with_a_macro_is_left_for_the_request() {
        let loaded = loaded("SecRule ARGS \"@rx ^%{tx.prefix}\\d+$\" \"id:1,pass\"\n");
        assert!(loaded.ruleset.phases[1][0].links[0].rx.is_none());
        assert_eq!(loaded.issues[0].code, "rx_macro");
        assert_eq!(loaded.issues[0].severity, IssueSeverity::Warning);
    }

    #[test]
    fn a_rule_without_an_id_is_not_loaded() {
        let loaded = loaded(concat!(
            "SecRule ARGS \"@rx a\" \"phase:1,pass\"\n",
            "SecAction \"phase:1,pass\"\n",
        ));
        assert_eq!(loaded.summary.rules, 0);
        assert_eq!(loaded.summary.skipped, 2);
        let seen: Vec<_> = loaded.issues.iter().map(|i| (i.line, i.code)).collect();
        assert_eq!(seen, [(1, "missing_id"), (2, "missing_id")]);
    }

    #[test]
    fn a_duplicate_id_keeps_the_first_rule() {
        let loaded = loaded(concat!(
            "SecRule ARGS \"@rx first\" \"id:1,pass\"\n",
            "SecAction \"id:1,pass\"\n",
        ));
        assert_eq!(loaded.ruleset.phases[1].len(), 1);
        assert_eq!(
            loaded.ruleset.phases[1][0].links[0].operator.argument,
            "first"
        );
        let [issue] = loaded.issues.as_slice() else {
            panic!("{:?}", loaded.issues)
        };
        assert_eq!(
            (issue.code, issue.line, issue.rule_id),
            ("duplicate_id", 2, Some(1))
        );
        assert_eq!(loaded.summary.skipped, 1);
    }

    #[test]
    fn needs_lists_what_enabled_rules_read() {
        let loaded = clean(&format!(
            "{CRS_911100}{}",
            concat!(
                "SecRule &ARGS_POST|!REQUEST_COOKIES:/x/ \"@eq 0\" \"id:1,pass,chain\"\n",
                "  SecRule XML:/* \"@rx a\" \"logdata:'%{request_headers.host}'\"\n",
                "SecRule FILES \"@rx a\" \"id:2,pass\"\n",
                "SecRuleRemoveById 2\n",
            )
        ));
        let needs: Vec<_> = loaded.ruleset.needs.iter().map(String::as_str).collect();
        assert_eq!(
            needs,
            [
                "ARGS_POST",
                "MATCHED_VAR",
                "REQUEST_HEADERS",
                "REQUEST_METHOD",
                "TX",
                "XML"
            ],
            "targets and macros count; exclusions and removed rules do not"
        );
    }

    #[test]
    fn the_bench_engine_config_loads_without_noise() {
        let loaded = clean(include_str!("../../bench/waf/main.conf"));
        let settings = &loaded.ruleset.settings;
        assert_eq!(settings.rule_engine, Some(RuleEngine::On));
        assert_eq!(settings.request_body_access, Some(true));
        assert_eq!(settings.request_body_limit, Some(262144));
        assert_eq!(settings.request_body_no_files_limit, Some(262144));
        assert_eq!(
            settings.request_body_limit_action,
            Some(BodyLimitAction::Reject)
        );
        assert_eq!(settings.response_body_access, Some(false));
        let rule = &loaded.ruleset.phases[0][0];
        assert_eq!(rule.id, 200001);
        assert!(
            rule.links[0]
                .actions
                .contains(&Action::Ctl("requestBodyProcessor".into(), "JSON".into()))
        );
        assert!(matches!(&rule.links[0].targets[0].key, Key::Name(n) if n == "Content-Type"));
    }

    #[test]
    fn signatures_and_timeouts_are_kept() {
        let loaded =
            clean("SecComponentSignature \"OWASP_CRS/4.21.0\"\nSecCollectionTimeout 600\n");
        assert_eq!(loaded.ruleset.component_signatures, ["OWASP_CRS/4.21.0"]);
        assert_eq!(loaded.ruleset.collection_timeout, Some(600));
    }

    #[test]
    fn files_load_in_the_order_given_and_globs_in_sorted_order() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.path().join(name), text).unwrap();
        write("setup.conf", "SecAction \"id:1,phase:1,pass\"\n");
        write("rules-20.conf", "SecAction \"id:20,phase:1,pass\"\n");
        write("rules-10.conf", "SecAction \"id:10,phase:1,pass\"\n");
        write(
            "rules-10.conf.example",
            "SecAction \"id:11,phase:1,pass\"\n",
        );
        write("binary.conf.bin", "");
        std::fs::write(dir.path().join("rules-30.conf"), b"SecMarker \xff\n").unwrap();

        let loaded = load(&[
            dir.path().join("setup.conf"),
            dir.path().join("rules-*.conf"),
            dir.path().join("missing.conf"),
            dir.path().join("nothing-*.conf"),
        ]);
        assert_eq!(ids(&loaded, 1), [1, 10, 20]);
        assert_eq!(loaded.summary.files, 3);
        let seen: Vec<_> = loaded
            .issues
            .iter()
            .map(|i| {
                (
                    i.code,
                    i.line,
                    i.file.rsplit('/').next().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("unreadable_file", 0, "rules-30.conf"),
                ("unreadable_file", 0, "missing.conf"),
                ("no_files", 0, "nothing-*.conf"),
            ]
        );
        assert!(loaded.ruleset.phases[0][1].file.ends_with("rules-10.conf"));
    }

    #[test]
    fn the_summary_is_loud_about_what_was_left_out() {
        let loaded = loaded(concat!(
            "SecRule ARGS \"@rx a\" \"id:1,pass\"\n",
            "SecRule ARGS \"@rx (?=a)\" \"id:2,pass\"\n",
            "SecRuleScript x.lua\n",
        ));
        let text = loaded.to_string();
        assert!(text.contains("1 rules loaded, 1 skipped"), "{text}");
        assert!(text.contains("SecRule 2"), "{text}");
        assert!(text.contains("error test.conf:2 [rule 2]"), "{text}");
        assert!(
            text.contains("error test.conf:3 unsupported directive"),
            "{text}"
        );

        let json = serde_json::to_value(&loaded.issues[0]).unwrap();
        assert_eq!(json["severity"], "error");
        assert_eq!(json["rule_id"], 2);
        assert_eq!(json["line"], 2);
    }
}
