//! SecLang text -> [`Directive`]s (T505).
//!
//! The grammar is small enough to read by hand: directives are lines, `\` at
//! the end of a line continues it, arguments are bare words or quoted strings,
//! and the actions argument is a comma-separated list whose values may be
//! single-quoted. The scope is "enough to run CRS", not the whole language —
//! a directive this parser does not know is reported and skipped, never fatal.
//!
//! Nothing is compiled here. Patterns stay text, so parsing is cheap, the
//! result compares with `==`, and a bad regex is [`super::load`]'s problem.

use std::collections::BTreeMap;

use super::Issue;
use super::rule::{
    Action, BodyLimitAction, Directive, DirectiveKind, Disruptive, IdRange, Link, Operator,
    RuleEngine, SEVERITIES, Selector, SetVar, SetVarOp, Setting, Variable, quotable,
};

/// Everything one file had to say.
#[derive(Debug, Default)]
pub struct Parsed {
    pub directives: Vec<Directive>,
    pub issues: Vec<Issue>,
    /// Every directive line seen, by name — including the ones that failed.
    pub counts: BTreeMap<String, usize>,
    /// Rules dropped because of an error. A chain counts once.
    pub skipped: usize,
}

/// Directives with a meaning here, in their canonical spelling (SecLang
/// directive names are case-insensitive).
const KNOWN: [&str; 15] = [
    "SecRule",
    "SecAction",
    "SecDefaultAction",
    "SecMarker",
    "SecComponentSignature",
    "SecRuleRemoveById",
    "SecRuleRemoveByTag",
    "SecRuleUpdateTargetById",
    "SecCollectionTimeout",
    "SecRuleEngine",
    "SecRequestBodyAccess",
    "SecRequestBodyLimit",
    "SecRequestBodyNoFilesLimit",
    "SecRequestBodyLimitAction",
    "SecResponseBodyAccess",
];

/// Logging directives: prx has its own audit trail (T509), so these are
/// accepted without comment rather than reported on every load.
const QUIET: [&str; 3] = ["SecAuditEngine", "SecAuditLog", "SecDebugLog"];

/// Parses one rules file. `file` is only used to address issues.
pub fn parse(file: &str, text: &str) -> Parsed {
    let mut parser = Parser {
        file,
        out: Parsed::default(),
        chain: None,
    };
    for (line, logical) in logical_lines(text) {
        parser.directive(line, &logical);
    }
    parser.close_chain("the file ends");
    parser.out
}

/// A rule whose last link said `chain`, waiting for the next `SecRule`.
struct OpenChain {
    line: u32,
    id: Option<u64>,
    links: Vec<Link>,
    /// A link failed: keep consuming the chain, then drop all of it. Dropping
    /// only the bad link would hand its neighbours a different meaning.
    broken: bool,
}

struct Parser<'a> {
    file: &'a str,
    out: Parsed,
    chain: Option<OpenChain>,
}

/// Why a directive was rejected. `code` is the stable half of an [`Issue`].
struct Reject {
    code: &'static str,
    message: String,
}

fn invalid(message: impl Into<String>) -> Reject {
    Reject {
        code: "invalid_directive",
        message: message.into(),
    }
}

impl Parser<'_> {
    fn issue(&mut self, line: u32, rule_id: Option<u64>, reject: Reject) {
        self.out.issues.push(Issue::error(
            reject.code,
            self.file,
            line,
            rule_id,
            reject.message,
        ));
    }

    fn directive(&mut self, line: u32, text: &str) {
        let text = text.trim();
        let (word, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        let name = KNOWN
            .iter()
            .find(|known| known.eq_ignore_ascii_case(word))
            .map_or(word, |known| *known);
        *self.out.counts.entry(name.to_string()).or_default() += 1;

        if name != "SecRule" && self.chain.is_some() {
            self.close_chain(&format!("{name} follows"));
        }
        let args = match split_args(rest) {
            Ok(args) => args,
            Err(message) => {
                // Whether an unreadable SecRule said `chain` is unknowable, so
                // an open chain cannot be trusted past it either.
                match (name, self.chain.take()) {
                    ("SecRule", Some(chain)) => {
                        let why = "the chain runs into a line that cannot be read; skipped";
                        self.drop_chain(chain, invalid(why));
                    }
                    ("SecRule" | "SecAction", _) => self.out.skipped += 1,
                    _ => {}
                }
                return self.issue(line, None, invalid(message));
            }
        };

        if name == "SecRule" {
            return self.sec_rule(line, &args);
        }
        match directive_kind(name, &args) {
            Ok(Some(kind)) => self.out.directives.push(Directive { line, kind }),
            Ok(None) => {}
            Err(reject) => {
                if name == "SecAction" {
                    self.out.skipped += 1;
                }
                self.issue(line, None, reject);
            }
        }
    }

    fn sec_rule(&mut self, line: u32, args: &[String]) {
        let parsed = parse_link(line, args);
        let (chained, own_id) = match &parsed {
            Ok(link) => (
                link.actions.contains(&Action::Chain),
                action_id(&link.actions),
            ),
            Err(error) => (error.chained, error.id),
        };
        // A link never has an id. One that does is a rule of its own behind a
        // stray `chain`: the stray chain is what gets dropped, not this rule.
        if self.chain.is_some() && own_id.is_some() {
            self.close_chain("the next SecRule carries its own id");
        }
        let is_head = self.chain.is_none();
        let mut chain = self.chain.take().unwrap_or(OpenChain {
            line,
            id: own_id,
            links: Vec::new(),
            broken: false,
        });

        match parsed {
            Ok(link) => chain.links.push(link),
            Err(LinkError { mut reject, .. }) => {
                if !is_head || chained {
                    reject.message.push_str("; the whole chain is skipped");
                }
                chain.broken = true;
                self.issue(line, chain.id, reject);
            }
        }

        if chained {
            self.chain = Some(chain);
        } else if chain.broken {
            self.out.skipped += 1;
        } else {
            self.out.directives.push(Directive {
                line: chain.line,
                kind: DirectiveKind::Rule(chain.links),
            });
        }
    }

    /// Anything but a `SecRule` after `chain` leaves the chain with no end.
    fn close_chain(&mut self, because: &str) {
        if let Some(chain) = self.chain.take() {
            let reject = Reject {
                code: "dangling_chain",
                message: format!("`chain` expects another SecRule, but {because}; skipped"),
            };
            self.drop_chain(chain, reject);
        }
    }

    fn drop_chain(&mut self, chain: OpenChain, reject: Reject) {
        self.out.skipped += 1;
        // A broken chain has already said why it is going.
        if !chain.broken {
            self.issue(chain.line, chain.id, reject);
        }
    }
}

fn action_id(actions: &[Action]) -> Option<u64> {
    actions.iter().find_map(|action| match action {
        Action::Id(id) => Some(*id),
        _ => None,
    })
}

/// Joins continued lines. Yields the 1-based number of the first physical
/// line and the text, which is never altered beyond dropping the `\`: runs of
/// spaces inside a pattern mean something.
fn logical_lines(text: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let mut open: Option<(u32, String)> = None;
    for (number, raw) in (1u32..).zip(text.lines()) {
        let (start, mut buffer) = match open.take() {
            Some(open) => open,
            None => {
                // Only a line that starts a directive can be a comment. A
                // comment is never continued: if `#SecAction \` swallowed the
                // line after it, commenting out one line could silently take a
                // live rule with it.
                let first = raw.trim_start();
                if first.is_empty() || first.starts_with('#') {
                    continue;
                }
                (number, String::new())
            }
        };
        let body = raw.trim_end();
        let backslashes = body.len() - body.trim_end_matches('\\').len();
        if backslashes % 2 == 1 {
            buffer.push_str(&body[..body.len() - 1]);
            open = Some((start, buffer));
        } else {
            buffer.push_str(raw);
            out.push((start, buffer));
        }
    }
    out.extend(open);
    out
}

/// Reads the quoted string whose opening quote is at `start`. Returns the
/// value and the index just past the closing quote.
///
/// `\<quote>` is the only escape. Every other backslash pair passes through
/// untouched, which is what lets a regex sit in a rules file unmangled.
fn scan_quoted(s: &str, start: usize) -> Result<(String, usize), String> {
    let bytes = s.as_bytes();
    let quote = bytes[start];
    let mut value = String::new();
    let mut segment = start + 1;
    let mut i = start + 1;
    // Only ASCII bytes are ever inspected or sliced at, so every index below
    // is a char boundary even when the text is not ASCII.
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if bytes[i + 1] == quote {
                value.push_str(&s[segment..i]);
                segment = i + 1;
            }
            i += 2;
        } else if bytes[i] == quote {
            value.push_str(&s[segment..i]);
            return Ok((value, i + 1));
        } else {
            i += 1;
        }
    }
    Err(format!("unterminated {} quote", char::from(quote)))
}

fn split_args(s: &str) -> Result<Vec<String>, String> {
    let bytes = s.as_bytes();
    let mut args = Vec::new();
    let mut i = 0;
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return Ok(args);
        }
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let (value, next) = scan_quoted(s, i)?;
            args.push(value);
            i = next;
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            // `a\"b\'c` or a trailing `\`: a value no quoting could write
            // back. Nothing real looks like this, and accepting it would mean
            // a rule that cannot be saved again once T509 edits it.
            if !quotable(&s[start..i]) {
                return Err(format!("`{}` needs quoting", &s[start..i]));
            }
            args.push(s[start..i].to_string());
        }
    }
}

fn directive_kind(name: &str, args: &[String]) -> Result<Option<DirectiveKind>, Reject> {
    let one = || match args {
        [only] => Ok(only.as_str()),
        _ => Err(invalid(format!("{name} takes exactly one argument"))),
    };
    let number = || {
        one()?
            .parse::<u64>()
            .map_err(|_| invalid(format!("{name} takes a number")))
    };
    let on_off = || match one()?.to_ascii_lowercase().as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(invalid(format!("{name} takes On or Off"))),
    };
    let rule = |message: String| Reject {
        code: "invalid_rule",
        message,
    };

    Ok(Some(match name {
        "SecAction" => DirectiveKind::Action(parse_actions(one()?).map_err(rule)?),
        "SecDefaultAction" => DirectiveKind::DefaultAction(parse_actions(one()?).map_err(rule)?),
        "SecMarker" => DirectiveKind::Marker(one()?.to_string()),
        "SecComponentSignature" => DirectiveKind::ComponentSignature(one()?.to_string()),
        "SecRuleRemoveById" => DirectiveKind::RemoveById(parse_ids(name, args)?),
        "SecRuleRemoveByTag" => DirectiveKind::RemoveByTag(one()?.to_string()),
        "SecRuleUpdateTargetById" => match args {
            [ids, variables] => DirectiveKind::UpdateTargetById(
                parse_ids(name, std::slice::from_ref(ids))?,
                parse_variables(variables).map_err(invalid)?,
            ),
            // The three-argument form replaces a target instead of adding
            // one. CRS never uses it.
            _ => return Err(invalid(format!("{name} takes an id and a target list"))),
        },
        "SecCollectionTimeout" => DirectiveKind::CollectionTimeout(number()?),
        "SecRuleEngine" => DirectiveKind::Setting(Setting::RuleEngine(
            match one()?.to_ascii_lowercase().as_str() {
                "on" => RuleEngine::On,
                "off" => RuleEngine::Off,
                "detectiononly" => RuleEngine::DetectionOnly,
                _ => return Err(invalid(format!("{name} takes On, Off or DetectionOnly"))),
            },
        )),
        "SecRequestBodyAccess" => DirectiveKind::Setting(Setting::RequestBodyAccess(on_off()?)),
        "SecRequestBodyLimit" => DirectiveKind::Setting(Setting::RequestBodyLimit(number()?)),
        "SecRequestBodyNoFilesLimit" => {
            DirectiveKind::Setting(Setting::RequestBodyNoFilesLimit(number()?))
        }
        "SecRequestBodyLimitAction" => DirectiveKind::Setting(Setting::RequestBodyLimitAction(
            match one()?.to_ascii_lowercase().as_str() {
                "reject" => BodyLimitAction::Reject,
                "processpartial" => BodyLimitAction::ProcessPartial,
                _ => return Err(invalid(format!("{name} takes Reject or ProcessPartial"))),
            },
        )),
        "SecResponseBodyAccess" => DirectiveKind::Setting(Setting::ResponseBodyAccess(on_off()?)),
        _ if QUIET.iter().any(|quiet| {
            let head = name.as_bytes().get(..quiet.len());
            head.is_some_and(|head| head.eq_ignore_ascii_case(quiet.as_bytes()))
        }) =>
        {
            return Ok(None);
        }
        // Includes SecRuleScript, which is out of scope for good.
        _ => {
            return Err(Reject {
                code: "unsupported_directive",
                message: format!("unsupported directive `{name}`; ignored"),
            });
        }
    }))
}

fn parse_ids(name: &str, args: &[String]) -> Result<Vec<IdRange>, Reject> {
    if args.is_empty() {
        return Err(invalid(format!("{name} takes at least one id")));
    }
    args.iter()
        .map(|arg| {
            let (from, to) = arg.split_once('-').unwrap_or((arg, arg));
            match (from.trim().parse(), to.trim().parse()) {
                (Ok(from), Ok(to)) if from <= to => Ok(IdRange { from, to }),
                _ => Err(invalid(format!(
                    "`{arg}` is not an id or an `a-b` id range"
                ))),
            }
        })
        .collect()
}

/// A `SecRule` line that did not parse, with as much of its identity as
/// could still be read: the chain it belongs to has to be settled either way.
struct LinkError {
    reject: Reject,
    chained: bool,
    id: Option<u64>,
}

fn parse_link(line: u32, args: &[String]) -> Result<Link, LinkError> {
    let error = |message: String, chained: bool, id: Option<u64>| LinkError {
        reject: Reject {
            code: "invalid_rule",
            message,
        },
        chained,
        id,
    };
    let (variables, operator, actions) = match args {
        [variables, operator] => (variables, operator, ""),
        [variables, operator, actions] => (variables, operator, actions.as_str()),
        _ => {
            let message = "SecRule takes a target list, an operator and an action list";
            return Err(error(message.into(), false, None));
        }
    };
    // Actions first: they say whether this is a chain link, and which rule to
    // blame if the rest does not parse.
    let actions = parse_actions(actions).map_err(|message| error(message, false, None))?;
    let (chained, id) = (actions.contains(&Action::Chain), action_id(&actions));
    Ok(Link {
        line,
        variables: parse_variables(variables).map_err(|m| error(m, chained, id))?,
        operator: parse_operator(operator).map_err(|m| error(m, chained, id))?,
        actions,
    })
}

fn parse_variables(s: &str) -> Result<Vec<Variable>, String> {
    let bytes = s.as_bytes();
    let mut variables = Vec::new();
    let mut i = 0;
    loop {
        let (mut exclude, mut count) = (false, false);
        while i < bytes.len() && matches!(bytes[i], b'!' | b'&') {
            exclude |= bytes[i] == b'!';
            count |= bytes[i] == b'&';
            i += 1;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        if start == i {
            return Err(format!("expected a variable name in `{s}`"));
        }
        let collection = s[start..i].to_ascii_uppercase();

        let mut selector = None;
        if i < bytes.len() && bytes[i] == b':' {
            i += 1;
            let text = if i < bytes.len() && bytes[i] == b'\'' {
                let (text, next) = scan_quoted(s, i)?;
                i = next;
                text
            } else {
                let start = i;
                while i < bytes.len() && bytes[i] != b'|' {
                    i += 1;
                }
                s[start..i].to_string()
            };
            if text.is_empty() {
                return Err(format!("`{collection}:` has an empty selector"));
            }
            selector = Some(
                match text.strip_prefix('/').and_then(|t| t.strip_suffix('/')) {
                    Some(pattern) => Selector::Regex(pattern.to_string()),
                    None => Selector::Name(text),
                },
            );
        }
        variables.push(Variable {
            collection,
            selector,
            exclude,
            count,
        });

        match bytes.get(i) {
            None => return Ok(variables),
            Some(b'|') => i += 1,
            Some(_) => return Err(format!("expected `|` between variables in `{s}`")),
        }
    }
}

/// `@name argument`, `!@name argument`, or a bare pattern, which is `@rx`.
///
/// Whitespace before the argument is dropped, also for a bare pattern. That
/// keeps every operator writable as `@name argument`; a pattern that has to
/// start with a space can say `\x20`.
fn parse_operator(s: &str) -> Result<Operator, String> {
    let s = s.trim_start();
    let (negated, s) = match s.strip_prefix('!') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, s),
    };
    let (name, argument) = match s.strip_prefix('@') {
        Some(rest) => rest.split_once(char::is_whitespace).unwrap_or((rest, "")),
        None => ("rx", s),
    };
    if name.is_empty() {
        return Err("`@` without an operator name".into());
    }
    Ok(Operator {
        name: name.to_string(),
        negated,
        argument: argument.trim_start().to_string(),
    })
}

/// `name`, `name:value` or `name:'quoted, value'`, separated by commas.
pub(super) fn parse_actions(s: &str) -> Result<Vec<Action>, String> {
    let bytes = s.as_bytes();
    let space = |i: usize| i < bytes.len() && bytes[i].is_ascii_whitespace();
    let mut actions = Vec::new();
    let mut i = 0;
    loop {
        while space(i) || bytes.get(i) == Some(&b',') {
            i += 1;
        }
        if i >= bytes.len() {
            return Ok(actions);
        }
        let start = i;
        while i < bytes.len() && !space(i) && !matches!(bytes[i], b':' | b',') {
            i += 1;
        }
        let name = &s[start..i];
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!("expected an action name at `{}`", &s[start..]));
        }

        let mut value = None;
        if bytes.get(i) == Some(&b':') {
            i += 1;
            while space(i) {
                i += 1;
            }
            if bytes.get(i) == Some(&b'\'') {
                let (text, next) = scan_quoted(s, i)?;
                i = next;
                while space(i) {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] != b',' {
                    return Err(format!("unexpected text after the value of `{name}`"));
                }
                value = Some(text);
            } else {
                let start = i;
                while i < bytes.len() && bytes[i] != b',' {
                    i += 1;
                }
                value = Some(s[start..i].trim().to_string());
            }
        }
        actions.push(parse_action(name, value)?);
    }
}

fn parse_action(name: &str, value: Option<String>) -> Result<Action, String> {
    let lower = name.to_ascii_lowercase();
    let flag = |action: Action| match value {
        None => Ok(action),
        Some(_) => Err(format!("action `{name}` takes no value")),
    };
    let text = || {
        value
            .clone()
            .ok_or_else(|| format!("action `{name}` needs a value"))
    };
    fn number<T: std::str::FromStr>(name: &str, value: String) -> Result<T, String> {
        value
            .parse()
            .map_err(|_| format!("`{name}:{value}` is not a valid number"))
    }

    Ok(match lower.as_str() {
        "id" => Action::Id(number(name, text()?)?),
        "phase" => Action::Phase(match text()?.to_ascii_lowercase().as_str() {
            "request" => 2,
            "response" => 4,
            "logging" => 5,
            other => match other.parse() {
                Ok(phase @ 1..=5) => phase,
                _ => return Err(format!("`phase:{other}` is not a phase (1-5)")),
            },
        }),
        "severity" => {
            let value = text()?;
            let named = SEVERITIES
                .iter()
                .position(|severity| severity.eq_ignore_ascii_case(&value));
            match named.or_else(|| value.parse().ok().filter(|level| *level < 8)) {
                Some(level) => Action::Severity(u8::try_from(level).unwrap_or(7)),
                None => return Err(format!("`severity:{value}` is not a severity")),
            }
        }
        "tag" => Action::Tag(text()?),
        "msg" => Action::Msg(text()?),
        "logdata" => Action::LogData(text()?),
        "ver" => Action::Ver(text()?),
        "t" => Action::Transform(text()?),
        "chain" => return flag(Action::Chain),
        "skipafter" => Action::SkipAfter(text()?),
        "skip" => Action::Skip(number(name, text()?)?),
        "setvar" => Action::SetVar(parse_setvar(&text()?)),
        "ctl" => {
            let value = text()?;
            let (option, value) = value.split_once('=').unwrap_or((&value, ""));
            Action::Ctl(option.to_string(), value.to_string())
        }
        "capture" => return flag(Action::Capture),
        "multimatch" => return flag(Action::MultiMatch),
        "pass" => return flag(Action::Disruptive(Disruptive::Pass)),
        "block" => return flag(Action::Disruptive(Disruptive::Block)),
        "deny" => return flag(Action::Disruptive(Disruptive::Deny)),
        "drop" => return flag(Action::Disruptive(Disruptive::Drop)),
        "allow" => Action::Disruptive(Disruptive::Allow(value)),
        "redirect" => Action::Disruptive(Disruptive::Redirect(text()?)),
        "status" => Action::Status(number(name, text()?)?),
        "log" => return flag(Action::Log),
        "nolog" => return flag(Action::NoLog),
        "auditlog" => return flag(Action::AuditLog),
        "noauditlog" => return flag(Action::NoAuditLog),
        "expirevar" => Action::ExpireVar(text()?),
        "initcol" => Action::InitCol(text()?),
        _ => Action::Other(name.to_string(), value),
    })
}

fn parse_setvar(s: &str) -> SetVar {
    let (target, op, value) = match (s.strip_prefix('!'), s.split_once('=')) {
        (Some(target), _) => (target, SetVarOp::Unset, ""),
        (None, None) => (s, SetVarOp::Set, "1"),
        (None, Some((target, value))) => match (value.strip_prefix('+'), value.strip_prefix('-')) {
            (Some(value), _) => (target, SetVarOp::Add, value),
            (_, Some(value)) => (target, SetVarOp::Sub, value),
            _ => (target, SetVarOp::Set, value),
        },
    };
    SetVar {
        target: target.to_string(),
        op,
        value: value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Vec<DirectiveKind> {
        let parsed = parse("test.conf", text);
        assert_eq!(parsed.issues, [], "{text}");
        parsed.directives.into_iter().map(|d| d.kind).collect()
    }

    fn one_rule(text: &str) -> Vec<Link> {
        match ok(text).as_slice() {
            [DirectiveKind::Rule(links)] => links.clone(),
            other => panic!("expected one rule, got {other:?}"),
        }
    }

    /// Line numbers move when a directive is rendered onto one line, so a
    /// round trip compares everything but them.
    fn without_lines(directives: &[Directive]) -> Vec<DirectiveKind> {
        let mut kinds: Vec<_> = directives.iter().map(|d| d.kind.clone()).collect();
        for kind in &mut kinds {
            if let DirectiveKind::Rule(links) = kind {
                links.iter_mut().for_each(|link| link.line = 0);
            }
        }
        kinds
    }

    fn assert_round_trip(text: &str) {
        let first = parse("test.conf", text);
        let rendered: Vec<String> = first.directives.iter().map(Directive::to_string).collect();
        let second = parse("test.conf", &rendered.join("\n"));
        assert_eq!(
            second.issues,
            [],
            "rendered text must parse:\n{rendered:#?}"
        );
        assert_eq!(
            without_lines(&first.directives),
            without_lines(&second.directives),
            "{text:?} rendered as {rendered:#?}"
        );
    }

    // CRS v4.21.0, rule 920100 (Apache-2.0), shortened only in its tag list.
    const CRS_920100: &str = r#"SecRule REQUEST_LINE "!@rx (?i)^(?:get /[^#\?]*(?:\?[^\s\x0b#]*)?(?:#[^\s\x0b]*)?|(?:connect (?:(?:[0-9]{1,3}\.){3}[0-9]{1,3}\.?(?::[0-9]+)?|[\--9A-Z_a-z]+:[0-9]+)|options \*|[a-z]{3,10}[\s\x0b]+(?:[0-9A-Z_a-z]{3,7}?://[\--9A-Z_a-z]*(?::[0-9]+)?)?/[^#\?]*(?:\?[^\s\x0b#]*)?(?:#[^\s\x0b]*)?)[\s\x0b]+[\.-9A-Z_a-z]+)$" \
    "id:920100,\
    phase:1,\
    block,\
    t:none,\
    msg:'Invalid HTTP Request Line',\
    logdata:'%{request_line}',\
    tag:'application-multi',\
    tag:'OWASP_CRS',\
    ver:'OWASP_CRS/4.21.0',\
    severity:'WARNING',\
    setvar:'tx.inbound_anomaly_score_pl1=+%{tx.warning_anomaly_score}'"
"#;

    // CRS v4.21.0, rule 920450 (Apache-2.0), tags shortened: a chain whose
    // second link has a regex selector and no id.
    const CRS_920450: &str = r#"SecRule REQUEST_HEADERS_NAMES "@rx ^.*$" \
    "id:920450,\
    phase:1,\
    block,\
    capture,\
    t:none,t:lowercase,\
    msg:'HTTP header is restricted by policy (%{MATCHED_VAR})',\
    logdata:'Restricted header detected: %{MATCHED_VAR}',\
    tag:'OWASP_CRS',\
    ver:'OWASP_CRS/4.21.0',\
    severity:'CRITICAL',\
    setvar:'tx.header_name_920450_%{tx.0}=/%{tx.0}/',\
    chain"
    SecRule TX:/^header_name_920450_/ "@within %{tx.restricted_headers_basic}" \
        "setvar:'tx.inbound_anomaly_score_pl1=+%{tx.critical_anomaly_score}'"
"#;

    // CRS v4.21.0, rule 920540 (Apache-2.0): a msg with escaped quotes.
    const CRS_920540_MSG: &str = r#"SecRule REQUEST_HEADERS:User-Agent "@rx ^UP" "id:920540,phase:1,pass,msg:'Request header x-up-devcap-post-charset detected in combination with prefix \'UP\' to User-Agent'""#;

    #[test]
    fn a_real_rule_parses_into_every_part() {
        let links = one_rule(CRS_920100);
        let [link] = links.as_slice() else {
            panic!("not a chain")
        };
        assert_eq!(link.line, 1);
        assert_eq!(
            link.variables,
            [Variable {
                collection: "REQUEST_LINE".into(),
                selector: None,
                exclude: false,
                count: false,
            }]
        );
        assert!(link.operator.negated);
        assert!(link.operator.is_rx());
        assert!(link.operator.argument.starts_with("(?i)^(?:get /[^#\\?]*"));
        assert!(link.operator.argument.ends_with("[\\.-9A-Z_a-z]+)$"));
        assert_eq!(link.actions.len(), 11);
        assert_eq!(link.actions[0], Action::Id(920100));
        assert_eq!(link.actions[1], Action::Phase(1));
        assert_eq!(link.actions[2], Action::Disruptive(Disruptive::Block));
        assert_eq!(link.actions[3], Action::Transform("none".into()));
        assert_eq!(
            link.actions[4],
            Action::Msg("Invalid HTTP Request Line".into())
        );
        assert_eq!(link.actions[5], Action::LogData("%{request_line}".into()));
        assert_eq!(link.actions[9], Action::Severity(4));
        assert_eq!(
            link.actions[10],
            Action::SetVar(SetVar {
                target: "tx.inbound_anomaly_score_pl1".into(),
                op: SetVarOp::Add,
                value: "%{tx.warning_anomaly_score}".into(),
            })
        );
    }

    #[test]
    fn a_chain_owns_the_rules_after_it() {
        let links = one_rule(CRS_920450);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].line, 1);
        assert_eq!(links[1].line, 14);
        assert!(links[0].actions.contains(&Action::Chain));
        assert_eq!(action_id(&links[1].actions), None);
        assert_eq!(
            links[1].variables[0].selector,
            Some(Selector::Regex("^header_name_920450_".into()))
        );
        assert_eq!(links[1].operator.name, "within");
        assert_eq!(links[1].operator.argument, "%{tx.restricted_headers_basic}");
    }

    #[test]
    fn continuations_keep_the_first_line_number_and_the_spacing() {
        let parsed = parse(
            "test.conf",
            "# comment\n\nSecRule ARGS \"@rx a  b\" \\\n  \"id:1,\\\n  pass\"\nSecMarker END\n",
        );
        assert_eq!(parsed.issues, []);
        assert_eq!(parsed.directives[0].line, 3);
        assert_eq!(parsed.directives[1].line, 6);
        let DirectiveKind::Rule(links) = &parsed.directives[0].kind else {
            panic!("expected a rule")
        };
        assert_eq!(links[0].operator.argument, "a  b");
        assert_eq!(parsed.counts["SecRule"], 1);
        assert_eq!(parsed.counts["SecMarker"], 1);
    }

    #[test]
    fn a_comment_never_swallows_the_line_after_it() {
        let kinds = ok("# SecAction \\\nSecMarker LIVE\n");
        assert_eq!(kinds, [DirectiveKind::Marker("LIVE".into())]);
    }

    #[test]
    fn crlf_files_parse() {
        let kinds = ok("SecMarker A\r\nSecAction \\\r\n  \"id:1,pass\"\r\n");
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn only_the_quote_itself_is_unescaped() {
        let links = one_rule(r#"SecRule ARGS "@rx \"\d+\\x\"" "id:1,pass""#);
        assert_eq!(links[0].operator.argument, r#""\d+\\x""#);

        let links = one_rule(r#"SecRule ARGS '@rx it\'s "\d"' "id:1,pass""#);
        assert_eq!(links[0].operator.argument, r#"it's "\d""#);
    }

    #[test]
    fn quoted_action_values_keep_commas_colons_and_escaped_quotes() {
        let links = one_rule(CRS_920540_MSG);
        assert_eq!(
            links[0].actions[3],
            Action::Msg(
                "Request header x-up-devcap-post-charset detected in combination with prefix 'UP' to User-Agent"
                    .into()
            )
        );

        let links = one_rule(r#"SecRule ARGS "@rx a" "id:1,msg:'a, b: c',tag:'x',pass""#);
        assert_eq!(links[0].actions[1], Action::Msg("a, b: c".into()));
        assert_eq!(links[0].actions[2], Action::Tag("x".into()));
        assert_eq!(links[0].actions[3], Action::Disruptive(Disruptive::Pass));
    }

    #[test]
    fn targets_carry_negation_count_and_selectors() {
        // Shape of CRS 942100's target list, plus a quoted selector with a `|`.
        let links = one_rule(
            r#"SecRule REQUEST_COOKIES|!REQUEST_COOKIES:/__utm/|&TX:crs_setup_version|XML:/*|REQUEST_HEADERS:'/(Content-Length|Transfer-Encoding)/' "@rx a" "id:1,pass""#,
        );
        let shape: Vec<_> = links[0]
            .variables
            .iter()
            .map(|v| {
                (
                    v.collection.as_str(),
                    v.selector.clone(),
                    v.exclude,
                    v.count,
                )
            })
            .collect();
        assert_eq!(
            shape,
            [
                ("REQUEST_COOKIES", None, false, false),
                (
                    "REQUEST_COOKIES",
                    Some(Selector::Regex("__utm".into())),
                    true,
                    false
                ),
                (
                    "TX",
                    Some(Selector::Name("crs_setup_version".into())),
                    false,
                    true
                ),
                ("XML", Some(Selector::Name("/*".into())), false, false),
                (
                    "REQUEST_HEADERS",
                    Some(Selector::Regex("(Content-Length|Transfer-Encoding)".into())),
                    false,
                    false
                ),
            ]
        );
    }

    #[test]
    fn operators_default_to_rx_and_may_be_negated() {
        let op = |text: &str| parse_operator(text).unwrap();
        assert_eq!((op("^a").name.as_str(), op("^a").negated), ("rx", false));
        assert_eq!(op("!^a").argument, "^a");
        assert!(op("!^a").negated);
        assert_eq!(op("!@within %{tx.x}").name, "within");
        assert_eq!(op("@detectSQLi").argument, "");
        assert_eq!(
            op("@pmFromFile  lfi-os-files.data").argument,
            "lfi-os-files.data"
        );
        assert!(parse_operator("@ foo").is_err());
    }

    #[test]
    fn actions_are_typed() {
        let actions = parse_actions(
            "id:'7',phase:request,severity:2,skipAfter:END,skip:3,status:403,deny,\
             ctl:ruleRemoveTargetById=942100;ARGS:foo,ctl:requestBodyProcessor=JSON,\
             setvar:tx.a,setvar:!tx.b,setvar:'tx.c=-5',setvar:tx.d=x,allow:phase,allow,\
             redirect:'http://example.com/?a=1,2',initcol:ip=%{remote_addr},\
             expirevar:ip.x=60,multiMatch,capture,nolog,auditlog,rev:'2',sanitiseMatched",
        )
        .unwrap();
        let setvar = |target: &str, op, value: &str| {
            Action::SetVar(SetVar {
                target: target.into(),
                op,
                value: value.into(),
            })
        };
        assert_eq!(
            actions,
            [
                Action::Id(7),
                Action::Phase(2),
                Action::Severity(2),
                Action::SkipAfter("END".into()),
                Action::Skip(3),
                Action::Status(403),
                Action::Disruptive(Disruptive::Deny),
                Action::Ctl("ruleRemoveTargetById".into(), "942100;ARGS:foo".into()),
                Action::Ctl("requestBodyProcessor".into(), "JSON".into()),
                setvar("tx.a", SetVarOp::Set, "1"),
                setvar("tx.b", SetVarOp::Unset, ""),
                setvar("tx.c", SetVarOp::Sub, "5"),
                setvar("tx.d", SetVarOp::Set, "x"),
                Action::Disruptive(Disruptive::Allow(Some("phase".into()))),
                Action::Disruptive(Disruptive::Allow(None)),
                Action::Disruptive(Disruptive::Redirect("http://example.com/?a=1,2".into())),
                Action::InitCol("ip=%{remote_addr}".into()),
                Action::ExpireVar("ip.x=60".into()),
                Action::MultiMatch,
                Action::Capture,
                Action::NoLog,
                Action::AuditLog,
                Action::Other("rev".into(), Some("2".into())),
                Action::Other("sanitiseMatched".into(), None),
            ]
        );
    }

    #[test]
    fn bad_action_values_are_errors_not_guesses() {
        for bad in [
            "id:abc",
            "id",
            "phase:9",
            "severity:LOUD",
            "status:x",
            "pass:1",
            "msg:'open",
            "msg:'a'b",
            ":x",
        ] {
            assert!(parse_actions(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_other_directive_parses() {
        let kinds = ok(concat!(
            "SecAction \"id:900990,phase:1,pass,t:none,nolog,setvar:tx.crs_setup_version=4210\"\n",
            "SecDefaultAction \"phase:1,log,auditlog,pass\"\n",
            "SecMarker \"BEGIN-HOST-CHECK\"\n",
            "SecComponentSignature \"OWASP_CRS/4.21.0\"\n",
            "SecRuleRemoveById 942100 \"920100-920199\" 1\n",
            "SecRuleRemoveByTag \"attack-injection-php\"\n",
            "SecRuleUpdateTargetById 942100 \"!ARGS:foo|!ARGS:/^bar/\"\n",
            "SecCollectionTimeout 600\n",
            "secruleengine detectiononly\n",
            "SecRequestBodyAccess On\n",
            "SecRequestBodyLimit 262144\n",
            "SecRequestBodyNoFilesLimit 262144\n",
            "SecRequestBodyLimitAction Reject\n",
            "SecResponseBodyAccess Off\n",
            "SecAuditEngine Off\n",
            "SecAuditLogParts ABIJDEFHZ\n",
            "SecDebugLogLevel 0\n",
        ));
        assert_eq!(
            kinds.len(),
            14,
            "the three logging directives leave nothing behind"
        );
        assert_eq!(
            kinds[4],
            DirectiveKind::RemoveById(vec![
                IdRange {
                    from: 942100,
                    to: 942100
                },
                IdRange {
                    from: 920100,
                    to: 920199
                },
                IdRange { from: 1, to: 1 },
            ])
        );
        assert_eq!(
            kinds[5],
            DirectiveKind::RemoveByTag("attack-injection-php".into())
        );
        assert!(
            matches!(&kinds[6], DirectiveKind::UpdateTargetById(ids, vars)
            if ids.len() == 1 && vars.len() == 2 && vars.iter().all(|v| v.exclude))
        );
        assert_eq!(kinds[7], DirectiveKind::CollectionTimeout(600));
        assert_eq!(
            kinds[8],
            DirectiveKind::Setting(Setting::RuleEngine(RuleEngine::DetectionOnly))
        );
        assert_eq!(
            kinds[12],
            DirectiveKind::Setting(Setting::RequestBodyLimitAction(BodyLimitAction::Reject))
        );
    }

    #[test]
    fn an_unknown_directive_is_reported_and_the_file_goes_on() {
        let parsed = parse(
            "x.conf",
            "SecMarker A\nSecRuleScript /x.lua \"id:5,deny\"\nInclude other.conf\nSecMarker B\n",
        );
        assert_eq!(parsed.directives.len(), 2);
        let seen: Vec<_> = parsed
            .issues
            .iter()
            .map(|i| (i.code, i.file.as_str(), i.line))
            .collect();
        assert_eq!(
            seen,
            [
                ("unsupported_directive", "x.conf", 2),
                ("unsupported_directive", "x.conf", 3)
            ]
        );
        assert!(parsed.issues[0].message.contains("SecRuleScript"));
        assert_eq!(parsed.counts["SecRuleScript"], 1);
    }

    #[test]
    fn a_broken_rule_is_reported_with_its_id_and_costs_only_itself() {
        let parsed = parse(
            "x.conf",
            "SecRule ARGS \"@rx a\" \"id:1,pass\"\n\
             SecRule ARGS:|x \"@rx a\" \"id:2,pass\"\n\
             SecRule ARGS \"@rx a\" \"id:3,phase:nine\"\n\
             SecRule ARGS \"@rx unterminated\n\
             SecRule ARGS\n\
             SecAction \"phase:1,pass:1\"\n\
             SecRuleRemoveById 1-x\n\
             SecRule ARGS \"@rx a\" \"id:9,pass\"\n",
        );
        assert_eq!(parsed.directives.len(), 2);
        let seen: Vec<_> = parsed
            .issues
            .iter()
            .map(|i| (i.line, i.code, i.rule_id))
            .collect();
        assert_eq!(
            seen,
            [
                (2, "invalid_rule", Some(2)),
                (3, "invalid_rule", None),
                (4, "invalid_directive", None),
                (5, "invalid_rule", None),
                (6, "invalid_rule", None),
                (7, "invalid_directive", None),
            ]
        );
        assert_eq!(parsed.skipped, 5);
    }

    #[test]
    fn a_dangling_chain_is_an_error_with_coordinates() {
        for text in [
            "SecMarker A\nSecRule ARGS \"@rx a\" \"id:7,chain\"\n",
            "SecMarker A\nSecRule ARGS \"@rx a\" \"id:7,chain\"\nSecMarker B\n",
        ] {
            let parsed = parse("x.conf", text);
            let [issue] = parsed.issues.as_slice() else {
                panic!("{:?}", parsed.issues)
            };
            assert_eq!(
                (issue.code, issue.line, issue.rule_id),
                ("dangling_chain", 2, Some(7))
            );
            assert_eq!(parsed.skipped, 1);
            assert!(
                parsed
                    .directives
                    .iter()
                    .all(|d| matches!(d.kind, DirectiveKind::Marker(_)))
            );
        }
    }

    #[test]
    fn a_broken_link_takes_the_whole_chain_and_nothing_else() {
        let parsed = parse(
            "x.conf",
            "SecRule ARGS \"@rx a\" \"id:1,chain\"\n\
             SecRule ARGS: \"@rx b\" \"chain\"\n\
             SecRule ARGS \"@rx c\"\n\
             SecRule ARGS \"@rx d\" \"id:2,pass\"\n",
        );
        let [issue] = parsed.issues.as_slice() else {
            panic!("{:?}", parsed.issues)
        };
        assert_eq!((issue.line, issue.rule_id), (2, Some(1)));
        assert!(issue.message.contains("whole chain"));
        assert_eq!(parsed.skipped, 1);
        let [
            Directive {
                line: 4,
                kind: DirectiveKind::Rule(links),
            },
        ] = parsed.directives.as_slice()
        else {
            panic!("{:?}", parsed.directives)
        };
        assert_eq!(action_id(&links[0].actions), Some(2));
    }

    #[test]
    fn an_unreadable_line_ends_the_chain_it_interrupts() {
        let parsed = parse(
            "x.conf",
            "SecRule ARGS \"@rx a\" \"id:1,chain\"\nSecRule ARGS \"@rx open\nSecMarker A\n",
        );
        let seen: Vec<_> = parsed.issues.iter().map(|i| (i.line, i.rule_id)).collect();
        assert_eq!(seen, [(1, Some(1)), (2, None)]);
        assert_eq!(parsed.skipped, 1);
        assert_eq!(
            parsed.directives.len(),
            1,
            "the marker after it is untouched"
        );
    }

    #[test]
    fn a_chained_rule_with_an_id_is_a_stray_chain() {
        let parsed = parse(
            "x.conf",
            "SecRule ARGS \"@rx a\" \"id:1,chain\"\nSecRule ARGS \"@rx b\" \"id:2,pass\"\n",
        );
        let [issue] = parsed.issues.as_slice() else {
            panic!("{:?}", parsed.issues)
        };
        assert_eq!(
            (issue.code, issue.line, issue.rule_id),
            ("dangling_chain", 1, Some(1))
        );
        let [
            Directive {
                line: 2,
                kind: DirectiveKind::Rule(links),
            },
        ] = parsed.directives.as_slice()
        else {
            panic!("{:?}", parsed.directives)
        };
        assert_eq!(action_id(&links[0].actions), Some(2));
    }

    #[test]
    fn rendering_round_trips() {
        assert_round_trip(CRS_920100);
        assert_round_trip(CRS_920450);
        assert_round_trip(CRS_920540_MSG);
        assert_round_trip(concat!(
            "SecRule REQUEST_COOKIES|!REQUEST_COOKIES:/__utm/|&TX:a|XML:/*|REQUEST_HEADERS:'/(a|b)/' \"!@rx \\\"x\\\"\\s\" \"id:1,phase:response,severity:2,msg:'it\\'s, fine',setvar:!tx.a,setvar:tx.b,allow,ctl:ruleEngine=Off,foo:'',bar\"\n",
            "SecRule ARGS '@rx it\\'s \"q\" \\d' \"id:2,pass\"\n",
            "SecRule ARGS @detectSQLi id:3,pass\n",
            "SecRule ARGS \"^implicit\" \"id:4,chain\"\n",
            "SecRule ARGS \"@eq 1\"\n",
            "SecAction \"id:5,phase:1,pass,nolog,initcol:ip=%{remote_addr}\"\n",
            "SecDefaultAction \"phase:2,log,auditlog,deny,status:403,t:lowercase\"\n",
            "SecMarker END-OF-THINGS\n",
            "SecMarker \"with space\"\n",
            "SecComponentSignature \"OWASP_CRS/4.21.0\"\n",
            "SecRuleRemoveById 1 5-9\n",
            "SecRuleRemoveByTag \"attack-(sqli|xss)\"\n",
            "SecRuleUpdateTargetById 1-3 \"!ARGS:foo|!ARGS:/^b a/\"\n",
            "SecCollectionTimeout 600\n",
            "SecRuleEngine DetectionOnly\n",
            "SecRequestBodyAccess On\n",
            "SecRequestBodyLimit 1\n",
            "SecRequestBodyNoFilesLimit 2\n",
            "SecRequestBodyLimitAction ProcessPartial\n",
            "SecResponseBodyAccess Off\n",
        ));
    }

    #[test]
    fn garbage_never_panics_and_whatever_parses_round_trips() {
        use rand::rngs::StdRng;
        use rand::{Rng, SeedableRng};

        let seeds = [
            CRS_920100,
            CRS_920450,
            CRS_920540_MSG,
            "SecRule REQUEST_COOKIES|!REQUEST_COOKIES:/__utm/|&TX:a|REQUEST_HEADERS:'/(a|b)/' \"!@rx \\\"x\\\"\" \"id:1,msg:'it\\'s, fine',setvar:!tx.a,chain\"\n  SecRule ARGS \"@eq 1\"\nSecRuleRemoveById 1 5-9\nSecRuleUpdateTargetById 1 \"!ARGS:foo\"\nSecRuleEngine On\n",
        ];
        let noise = b"\"'\\,:|!&@/# \t\n\r%{}=+-";
        let mut rng = StdRng::seed_from_u64(0x5ec1a96);
        let started = std::time::Instant::now();

        // Deterministic, and sized to stay well under a second in a debug
        // build: this rides along with every `make test`.
        const ROUNDS: usize = 2000;
        for round in 0..ROUNDS {
            let mut bytes = seeds[round % seeds.len()].as_bytes().to_vec();
            if round % 5 == 4 {
                bytes = (0..rng.random_range(0..200))
                    .map(|_| rng.random())
                    .collect();
            }
            for _ in 0..rng.random_range(1..6) {
                if bytes.is_empty() {
                    break;
                }
                let at = rng.random_range(0..bytes.len());
                match rng.random_range(0..5) {
                    0 => bytes.truncate(at),
                    1 => bytes[at] = noise[rng.random_range(0..noise.len())],
                    2 => bytes.insert(at, noise[rng.random_range(0..noise.len())]),
                    3 => bytes[at] = rng.random(),
                    _ => {
                        bytes.remove(at);
                    }
                }
            }
            let text = String::from_utf8_lossy(&bytes);
            assert_round_trip(&text);
            // The same text through the whole loader: compile must not panic
            // on anything the parser lets through.
            let _ = crate::waf::load_text(&[("fuzz.conf", &text)]);
        }
        println!("fuzz: {ROUNDS} inputs in {:?}", started.elapsed());
    }
}
