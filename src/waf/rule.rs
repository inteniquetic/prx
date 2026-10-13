//! The SecLang rule model (T505).
//!
//! Two layers live here. The first is what a rules file *says*: [`Directive`]
//! and the types under it, produced by [`super::parse::parse`] and written back
//! out by their `Display` impls. Nothing in it is resolved or compiled, so it
//! compares with `==` and survives a trip through text.
//!
//! The second is what the engine *runs*: [`RuleSet`], built once per load by
//! [`super::load`]. Defaults are inherited, patterns are compiled, jumps are
//! indices. It is immutable after that, so a reload is an `Arc` swap and the
//! request path never parses, compiles or resolves anything.

use crate::waf::prefilter::Prefilter;
use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// What the file says
// ---------------------------------------------------------------------------

/// One directive and the line it starts on (1-based; a continued directive is
/// addressed by its first line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub line: u32,
    pub kind: DirectiveKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectiveKind {
    /// `SecRule`. More than one link is a chain: the first link carries the
    /// id, phase and disruptive action, and the rule matches only when every
    /// link does.
    Rule(Vec<Link>),
    /// `SecAction`: a rule with no condition.
    Action(Vec<Action>),
    DefaultAction(Vec<Action>),
    Marker(String),
    ComponentSignature(String),
    RemoveById(Vec<IdRange>),
    /// An exact tag, as ModSecurity v3 and Coraza read it. (v2 reads a regex;
    /// a v2-style pattern here removes nothing, and the load says so.)
    RemoveByTag(String),
    UpdateTargetById(Vec<IdRange>, Vec<Variable>),
    CollectionTimeout(u64),
    Setting(Setting),
}

/// One `SecRule` line: a whole rule, or one link of a chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub line: u32,
    pub variables: Vec<Variable>,
    pub operator: Operator,
    pub actions: Vec<Action>,
}

/// One entry of a rule's target list, e.g. `ARGS`, `!ARGS:foo`, `&TX:score`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// Upper-cased: SecLang variable names are case-insensitive.
    pub collection: String,
    pub selector: Option<Selector>,
    /// `!` — takes matching entries *out* of the target list.
    pub exclude: bool,
    /// `&` — the number of entries rather than their values.
    pub count: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// `ARGS:foo`. Also what an XPath ends up as (`XML:/*`): it is not a
    /// regex, and T506 is the one that knows what to do with it.
    Name(String),
    /// `ARGS:/^foo/`, without the slashes.
    Regex(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operator {
    /// As written, without the `@`. `rx` when the rule names no operator.
    pub name: String,
    pub negated: bool,
    /// Raw: macros such as `%{tx.foo}` are expanded per request (T506).
    pub argument: String,
}

impl Operator {
    pub fn is_rx(&self) -> bool {
        self.name.eq_ignore_ascii_case("rx")
    }
}

/// An inclusive id range; a single id is a range of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdRange {
    pub from: u64,
    pub to: u64,
}

impl IdRange {
    pub fn contains(&self, id: u64) -> bool {
        (self.from..=self.to).contains(&id)
    }
}

/// The actions that carry semantics are typed; everything else lands in
/// [`Action::Other`] so that nothing a rule says is silently dropped. String
/// values stay raw, macros included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Id(u64),
    /// 1–5. `request`, `response` and `logging` are read as 2, 4 and 5.
    Phase(u8),
    /// 0 (EMERGENCY) – 7 (DEBUG).
    Severity(u8),
    Tag(String),
    Msg(String),
    LogData(String),
    Ver(String),
    /// `t:` — one transformation, as written.
    Transform(String),
    Chain,
    SkipAfter(String),
    Skip(u32),
    SetVar(SetVar),
    /// `ctl:option=value`.
    Ctl(String, String),
    Capture,
    MultiMatch,
    Disruptive(Disruptive),
    Status(u16),
    Log,
    NoLog,
    AuditLog,
    NoAuditLog,
    ExpireVar(String),
    InitCol(String),
    Other(String, Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disruptive {
    Pass,
    /// "Whatever `SecDefaultAction` says". Never appears in a compiled
    /// [`Rule::disruptive`]: it is resolved at load.
    Block,
    Deny,
    Drop,
    /// `allow`, `allow:phase`, `allow:request`.
    Allow(Option<String>),
    Redirect(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetVar {
    /// `tx.score`, macros and all.
    pub target: String,
    pub op: SetVarOp,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetVarOp {
    /// `tx.a=v`; a bare `tx.a` is `tx.a=1`.
    Set,
    /// `tx.a=+v`
    Add,
    /// `tx.a=-v`
    Sub,
    /// `!tx.a`
    Unset,
}

/// An engine directive, as opposed to a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    RuleEngine(RuleEngine),
    RequestBodyAccess(bool),
    RequestBodyLimit(u64),
    RequestBodyNoFilesLimit(u64),
    RequestBodyLimitAction(BodyLimitAction),
    ResponseBodyAccess(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleEngine {
    On,
    Off,
    DetectionOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyLimitAction {
    Reject,
    ProcessPartial,
}

pub(super) const SEVERITIES: [&str; 8] = [
    "EMERGENCY",
    "ALERT",
    "CRITICAL",
    "ERROR",
    "WARNING",
    "NOTICE",
    "INFO",
    "DEBUG",
];

// ---------------------------------------------------------------------------
// Writing it back out
// ---------------------------------------------------------------------------

/// `s` inside `quote`s, or `None` when that quoting cannot express it.
///
/// The only escape a quoted SecLang string has is `\<quote>`; every other
/// backslash is content (CRS writes a literal backslash as `\x5c` for exactly
/// this reason). So a quote that already follows an odd run of backslashes, or
/// a value that ends in one, has no spelling in that quote style.
fn quoted(s: &str, quote: char) -> Option<String> {
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    let mut backslashes = 0;
    for c in s.chars() {
        if c == quote {
            if backslashes % 2 == 1 {
                return None;
            }
            out.push('\\');
        }
        backslashes = if c == '\\' { backslashes + 1 } else { 0 };
        out.push(c);
    }
    (backslashes % 2 == 0).then(|| {
        out.push(quote);
        out
    })
}

/// Whether some quoting can express `s`. The parser holds bare arguments to
/// this, so that whatever it accepts can be written back.
pub(super) fn quotable(s: &str) -> bool {
    quoted(s, '"').or_else(|| quoted(s, '\'')).is_some()
}

/// A directive argument. Anything the parser can produce has at least one
/// spelling: a double-quoted argument is never unsafe for double quotes, a
/// single-quoted one never for single quotes, and a bare one is [`quotable`].
fn arg(s: &str, prefer_bare: bool) -> String {
    let bare_ok = !s.is_empty() && !s.contains(|c: char| c.is_whitespace() || "\"'\\".contains(c));
    if prefer_bare && bare_ok {
        return s.to_string();
    }
    quoted(s, '"')
        .or_else(|| quoted(s, '\''))
        .unwrap_or_else(|| s.to_string())
}

/// An action value: bare when that reads back the same, single-quoted if not.
fn action_value(s: &str) -> String {
    let bare_ok = !s.is_empty() && !s.starts_with('\'') && !s.contains(',') && s.trim() == s;
    match quoted(s, '\'') {
        Some(quoted) if !bare_ok => quoted,
        _ => s.to_string(),
    }
}

fn join<T: fmt::Display>(items: &[T], separator: &str) -> String {
    let parts: Vec<String> = items.iter().map(T::to_string).collect();
    parts.join(separator)
}

fn actions_arg(actions: &[Action]) -> String {
    let mut list = join(actions, ",");
    // `msg:a\` cannot end a quoted argument: the backslash would take the
    // closing quote. A trailing comma is free, and keeps it off the end.
    if list.ends_with('\\') {
        list.push(',');
    }
    arg(&list, false)
}

impl fmt::Display for Directive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl fmt::Display for DirectiveKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rule(links) => {
                for (n, link) in links.iter().enumerate() {
                    // Chained links are separate SecRule lines; the indent is
                    // how CRS writes them and means nothing to the parser.
                    let indent = if n == 0 { "" } else { "\n    " };
                    write!(f, "{indent}{link}")?;
                }
                Ok(())
            }
            Self::Action(actions) => write!(f, "SecAction {}", actions_arg(actions)),
            Self::DefaultAction(actions) => {
                write!(f, "SecDefaultAction {}", actions_arg(actions))
            }
            Self::Marker(name) => write!(f, "SecMarker {}", arg(name, true)),
            Self::ComponentSignature(text) => {
                write!(f, "SecComponentSignature {}", arg(text, false))
            }
            Self::RemoveById(ids) => write!(f, "SecRuleRemoveById {}", join(ids, " ")),
            Self::RemoveByTag(tag) => write!(f, "SecRuleRemoveByTag {}", arg(tag, false)),
            Self::UpdateTargetById(ids, variables) => write!(
                f,
                "SecRuleUpdateTargetById {} {}",
                join(ids, " "),
                arg(&join(variables, "|"), true)
            ),
            Self::CollectionTimeout(seconds) => write!(f, "SecCollectionTimeout {seconds}"),
            Self::Setting(setting) => setting.fmt(f),
        }
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SecRule {} {}",
            arg(&join(&self.variables, "|"), true),
            arg(&self.operator.to_string(), false)
        )?;
        if !self.actions.is_empty() {
            write!(f, " {}", actions_arg(&self.actions))?;
        }
        Ok(())
    }
}

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.exclude {
            f.write_str("!")?;
        }
        if self.count {
            f.write_str("&")?;
        }
        f.write_str(&self.collection)?;
        let selector = match &self.selector {
            None => return Ok(()),
            Some(Selector::Name(name)) => name.clone(),
            Some(Selector::Regex(pattern)) => format!("/{pattern}/"),
        };
        // A bare selector ends at the next `|`; a quoted one does not.
        match quoted(&selector, '\'') {
            Some(quoted) if selector.contains('|') || selector.starts_with('\'') => {
                write!(f, ":{quoted}")
            }
            _ => write!(f, ":{selector}"),
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negated {
            f.write_str("!")?;
        }
        write!(f, "@{}", self.name)?;
        if !self.argument.is_empty() {
            write!(f, " {}", self.argument)?;
        }
        Ok(())
    }
}

impl fmt::Display for IdRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.from == self.to {
            write!(f, "{}", self.from)
        } else {
            write!(f, "{}-{}", self.from, self.to)
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, value): (&str, Option<String>) = match self {
            Self::Id(id) => ("id", Some(id.to_string())),
            Self::Phase(phase) => ("phase", Some(phase.to_string())),
            Self::Severity(level) => (
                "severity",
                Some(
                    SEVERITIES
                        .get(usize::from(*level))
                        .map_or_else(|| level.to_string(), |name| name.to_string()),
                ),
            ),
            Self::Tag(v) => ("tag", Some(v.clone())),
            Self::Msg(v) => ("msg", Some(v.clone())),
            Self::LogData(v) => ("logdata", Some(v.clone())),
            Self::Ver(v) => ("ver", Some(v.clone())),
            Self::Transform(v) => ("t", Some(v.clone())),
            Self::Chain => ("chain", None),
            Self::SkipAfter(v) => ("skipAfter", Some(v.clone())),
            Self::Skip(n) => ("skip", Some(n.to_string())),
            Self::SetVar(setvar) => ("setvar", Some(setvar.to_string())),
            Self::Ctl(option, v) => ("ctl", Some(format!("{option}={v}"))),
            Self::Capture => ("capture", None),
            Self::MultiMatch => ("multiMatch", None),
            Self::Disruptive(Disruptive::Pass) => ("pass", None),
            Self::Disruptive(Disruptive::Block) => ("block", None),
            Self::Disruptive(Disruptive::Deny) => ("deny", None),
            Self::Disruptive(Disruptive::Drop) => ("drop", None),
            Self::Disruptive(Disruptive::Allow(v)) => ("allow", v.clone()),
            Self::Disruptive(Disruptive::Redirect(v)) => ("redirect", Some(v.clone())),
            Self::Status(code) => ("status", Some(code.to_string())),
            Self::Log => ("log", None),
            Self::NoLog => ("nolog", None),
            Self::AuditLog => ("auditlog", None),
            Self::NoAuditLog => ("noauditlog", None),
            Self::ExpireVar(v) => ("expirevar", Some(v.clone())),
            Self::InitCol(v) => ("initcol", Some(v.clone())),
            Self::Other(name, v) => (name.as_str(), v.clone()),
        };
        match value {
            Some(value) => write!(f, "{name}:{}", action_value(&value)),
            None => f.write_str(name),
        }
    }
}

impl fmt::Display for SetVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { target, op, value } = self;
        match op {
            SetVarOp::Set => write!(f, "{target}={value}"),
            SetVarOp::Add => write!(f, "{target}=+{value}"),
            SetVarOp::Sub => write!(f, "{target}=-{value}"),
            SetVarOp::Unset => write!(f, "!{target}"),
        }
    }
}

impl fmt::Display for Setting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let on_off = |on: &bool| if *on { "On" } else { "Off" };
        match self {
            Self::RuleEngine(RuleEngine::On) => f.write_str("SecRuleEngine On"),
            Self::RuleEngine(RuleEngine::Off) => f.write_str("SecRuleEngine Off"),
            Self::RuleEngine(RuleEngine::DetectionOnly) => {
                f.write_str("SecRuleEngine DetectionOnly")
            }
            Self::RequestBodyAccess(on) => write!(f, "SecRequestBodyAccess {}", on_off(on)),
            Self::RequestBodyLimit(n) => write!(f, "SecRequestBodyLimit {n}"),
            Self::RequestBodyNoFilesLimit(n) => write!(f, "SecRequestBodyNoFilesLimit {n}"),
            Self::RequestBodyLimitAction(BodyLimitAction::Reject) => {
                f.write_str("SecRequestBodyLimitAction Reject")
            }
            Self::RequestBodyLimitAction(BodyLimitAction::ProcessPartial) => {
                f.write_str("SecRequestBodyLimitAction ProcessPartial")
            }
            Self::ResponseBodyAccess(on) => write!(f, "SecResponseBodyAccess {}", on_off(on)),
        }
    }
}

// ---------------------------------------------------------------------------
// What the engine runs
// ---------------------------------------------------------------------------

/// A pattern compiled at load.
#[derive(Debug, Clone)]
pub struct Rx {
    /// On bytes: request data is not required to be UTF-8.
    pub regex: regex::bytes::Regex,
    /// True when the pattern as written was rejected and only the PCRE repair
    /// layer made it compile. Kept so that a repaired security control is
    /// never invisible.
    pub repaired: bool,
}

/// Engine directives. `None` is "the rules files did not say", which is not
/// the same as off: the plugin config (T506/T509) decides what that means.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub rule_engine: Option<RuleEngine>,
    pub request_body_access: Option<bool>,
    pub request_body_limit: Option<u64>,
    pub request_body_no_files_limit: Option<u64>,
    pub request_body_limit_action: Option<BodyLimitAction>,
    pub response_body_access: Option<bool>,
}

impl Settings {
    pub(super) fn apply(&mut self, setting: Setting) {
        match setting {
            Setting::RuleEngine(v) => self.rule_engine = Some(v),
            Setting::RequestBodyAccess(v) => self.request_body_access = Some(v),
            Setting::RequestBodyLimit(v) => self.request_body_limit = Some(v),
            Setting::RequestBodyNoFilesLimit(v) => self.request_body_no_files_limit = Some(v),
            Setting::RequestBodyLimitAction(v) => self.request_body_limit_action = Some(v),
            Setting::ResponseBodyAccess(v) => self.response_body_access = Some(v),
        }
    }
}

/// Values from `[waf]` that the rules read as TX variables. The evaluator
/// writes them into the transaction before phase 1 runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tuning {
    /// `tx.blocking_paranoia_level`: rules at or below this level score toward blocking.
    pub paranoia_level: u8,
    /// `tx.inbound_anomaly_score_threshold`.
    pub inbound_anomaly_threshold: u32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            paranoia_level: crate::config::DEFAULT_PARANOIA_LEVEL,
            inbound_anomaly_threshold: crate::config::DEFAULT_INBOUND_ANOMALY_THRESHOLD,
        }
    }
}

/// An immutable, fully resolved ruleset.
#[derive(Debug, Default, Clone)]
pub struct RuleSet {
    pub settings: Settings,
    pub tuning: Tuning,
    /// Rules by phase (index 0 is phase 1), in file order within a phase.
    pub phases: [Vec<Rule>; 5],
    /// Every distinct transformation chain in the ruleset, lower-cased and
    /// with defaults and `t:none` already applied. [`RuleLink::transforms`]
    /// indexes it, which makes that index the memoisation key for
    /// "this variable, transformed this way" within a request.
    pub transform_chains: Vec<Vec<String>>,
    /// Collections some enabled rule reads, as a target or through a macro.
    /// A collection that is not in here never has to be built for a rule to
    /// read. Writes are not listed: `setvar:tx.x=1` or `initcol:ip=...` into
    /// a collection nothing reads has no effect on any verdict, so the
    /// evaluator may drop such a write, but it must not assume the collection
    /// exists.
    pub needs: BTreeSet<String>,
    pub component_signatures: Vec<String>,
    pub collection_timeout: Option<u64>,
    pub prefilter: Option<Arc<Prefilter>>,
}

/// One rule with its whole chain, defaults applied.
#[derive(Debug, Clone)]
pub struct Rule {
    pub file: Arc<str>,
    pub line: u32,
    pub id: u64,
    pub phase: u8,
    /// Never [`Disruptive::Block`].
    pub disruptive: Disruptive,
    pub status: Option<u16>,
    pub log: bool,
    pub audit_log: bool,
    /// Where `skipAfter` lands: an index into this rule's own phase in
    /// [`RuleSet::phases`], possibly one past the end.
    pub skip_after: Option<usize>,
    /// The first link is the rule itself; the rest is its chain.
    pub links: Vec<RuleLink>,
}

impl Rule {
    pub fn tags(&self) -> impl Iterator<Item = &str> {
        self.links[0]
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Tag(tag) => Some(tag.as_str()),
                _ => None,
            })
    }
}

#[derive(Debug, Clone)]
pub struct RuleLink {
    pub line: u32,
    pub targets: Vec<Target>,
    /// `@unconditionalMatch` for a `SecAction`.
    pub operator: Operator,
    /// The compiled pattern of an `@rx`. `None` for every other operator, and
    /// for an `@rx` whose pattern holds a macro and so has no load-time form.
    pub rx: Option<Rx>,
    /// Index into [`RuleSet::transform_chains`].
    pub transforms: usize,
    /// As written, so nothing the rule says is lost to the typed fields.
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub collection: String,
    pub key: Key,
    pub exclude: bool,
    pub count: bool,
    /// What this target selects, as one string, so a per-request cache can be
    /// looked up without building a key. Equal targets share it.
    pub select_id: String,
    /// The same for the whole collection, which the selection is filtered from.
    pub collection_id: String,
}

#[derive(Debug, Clone)]
pub enum Key {
    /// The whole collection.
    All,
    Name(String),
    /// Matched against entry names, case-insensitively.
    Regex(Rx),
}
