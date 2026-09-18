//! Precompiled header rules (T106).
//!
//! Header names are parsed and value templates are split into literal and
//! variable segments once per config reload, so applying a rule to a request
//! is a walk over a small vector with no parsing and no formatting machinery.

use std::fmt::Write as _;

use http::header::{HeaderName, HeaderValue};
use pingora::http::{RequestHeader, ResponseHeader};

use crate::config::{HeaderRules, extract_variables};

/// One piece of a header value template.
#[derive(Debug, Clone)]
enum Segment {
    Literal(Box<str>),
    Variable(Variable),
}

/// The values a header template can interpolate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variable {
    ClientIp,
    ClientPort,
    Scheme,
    Host,
    RouteName,
    UpstreamAddr,
    RequestId,
}

impl Variable {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "client_ip" => Self::ClientIp,
            "client_port" => Self::ClientPort,
            "scheme" => Self::Scheme,
            "host" => Self::Host,
            "route_name" => Self::RouteName,
            "upstream_addr" => Self::UpstreamAddr,
            "request_id" => Self::RequestId,
            _ => return None,
        })
    }
}

/// What the caller can supply when a template is rendered. Everything is
/// borrowed: rendering a header must not force the request path to build
/// strings it does not already have.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeaderContext<'a> {
    pub client_ip: Option<&'a str>,
    pub client_port: Option<u16>,
    pub scheme: &'a str,
    pub host: Option<&'a str>,
    pub route_name: Option<&'a str>,
    pub upstream_addr: Option<&'a str>,
    pub request_id: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct Template {
    segments: Vec<Segment>,
    /// A template with no variables is rendered once at load time.
    constant: Option<HeaderValue>,
}

impl Template {
    fn compile(value: &str) -> Self {
        let mut segments = Vec::new();
        let mut rest = value;

        while let Some(pos) = rest.find('$') {
            let (literal, tail) = rest.split_at(pos);
            if !literal.is_empty() {
                segments.push(Segment::Literal(literal.into()));
            }
            let name_len = tail[1..]
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
            match Variable::parse(&tail[1..1 + name_len]) {
                Some(variable) if name_len > 0 => segments.push(Segment::Variable(variable)),
                // Not a known variable: keep the text as written. Config
                // validation already rejected unknown variables, so this only
                // happens for a bare '$'.
                _ => segments.push(Segment::Literal(tail[..1 + name_len].into())),
            }
            rest = &tail[1 + name_len..];
        }
        if !rest.is_empty() {
            segments.push(Segment::Literal(rest.into()));
        }

        let constant = if segments
            .iter()
            .all(|segment| matches!(segment, Segment::Literal(_)))
        {
            HeaderValue::from_str(value).ok()
        } else {
            None
        };

        Self { segments, constant }
    }

    fn render(&self, ctx: &HeaderContext<'_>) -> Option<HeaderValue> {
        if let Some(constant) = &self.constant {
            return Some(constant.clone());
        }

        let mut out = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => out.push_str(text),
                Segment::Variable(variable) => match variable {
                    Variable::ClientIp => out.push_str(ctx.client_ip?),
                    Variable::ClientPort => {
                        let _ = write!(out, "{}", ctx.client_port?);
                    }
                    Variable::Scheme => out.push_str(ctx.scheme),
                    Variable::Host => out.push_str(ctx.host?),
                    Variable::RouteName => out.push_str(ctx.route_name?),
                    Variable::UpstreamAddr => out.push_str(ctx.upstream_addr?),
                    Variable::RequestId => out.push_str(ctx.request_id?),
                },
            }
        }
        HeaderValue::from_str(&out).ok()
    }

    fn uses(&self, variable: Variable) -> bool {
        self.segments
            .iter()
            .any(|segment| matches!(segment, Segment::Variable(v) if *v == variable))
    }
}

/// Where a rule set writes its headers.
///
/// Pingora keeps a case-preserving name map next to the `HeaderMap` and
/// serializes HTTP/1.1 from it, so writing straight into `headers` is silently
/// dropped on the wire. Everything must go through its own methods; this trait
/// keeps that detail in one place and lets the unit tests use a plain
/// `HeaderMap`.
pub trait HeaderSink {
    fn set_header(&mut self, name: &HeaderName, value: HeaderValue);
    fn add_header(&mut self, name: &HeaderName, value: HeaderValue);
    fn remove_header_named(&mut self, name: &HeaderName);
}

impl HeaderSink for RequestHeader {
    fn set_header(&mut self, name: &HeaderName, value: HeaderValue) {
        let _ = self.insert_header(name.clone(), value);
    }

    fn add_header(&mut self, name: &HeaderName, value: HeaderValue) {
        let _ = self.append_header(name.clone(), value);
    }

    fn remove_header_named(&mut self, name: &HeaderName) {
        self.remove_header(name);
    }
}

impl HeaderSink for ResponseHeader {
    fn set_header(&mut self, name: &HeaderName, value: HeaderValue) {
        let _ = self.insert_header(name.clone(), value);
    }

    fn add_header(&mut self, name: &HeaderName, value: HeaderValue) {
        let _ = self.append_header(name.clone(), value);
    }

    fn remove_header_named(&mut self, name: &HeaderName) {
        self.remove_header(name);
    }
}

impl HeaderSink for http::HeaderMap {
    fn set_header(&mut self, name: &HeaderName, value: HeaderValue) {
        self.insert(name, value);
    }

    fn add_header(&mut self, name: &HeaderName, value: HeaderValue) {
        self.append(name, value);
    }

    fn remove_header_named(&mut self, name: &HeaderName) {
        self.remove(name);
    }
}

#[derive(Debug, Clone)]
enum Op {
    Set(HeaderName, Template),
    Add(HeaderName, Template),
    Remove(HeaderName),
}

/// A compiled rule set, ready to apply to a header map.
#[derive(Debug, Clone, Default)]
pub struct CompiledHeaderRules {
    ops: Vec<Op>,
}

impl CompiledHeaderRules {
    /// Compiles config rules. Invalid names are skipped: `PrxConfig::validate`
    /// rejects them before a config ever gets here.
    pub fn compile(rules: &HeaderRules) -> Self {
        let mut ops = Vec::with_capacity(rules.set.len() + rules.add.len() + rules.remove.len());

        for name in &rules.remove {
            if let Ok(name) = HeaderName::from_bytes(name.as_bytes()) {
                ops.push(Op::Remove(name));
            }
        }
        for (name, value) in &rules.set {
            if let Ok(name) = HeaderName::from_bytes(name.as_bytes()) {
                ops.push(Op::Set(name, Template::compile(value)));
            }
        }
        for (name, value) in &rules.add {
            if let Ok(name) = HeaderName::from_bytes(name.as_bytes()) {
                ops.push(Op::Add(name, Template::compile(value)));
            }
        }

        Self { ops }
    }

    /// Merges global rules with route rules; global ones apply first.
    pub fn merge(global: &Self, route: &Self) -> Self {
        let mut ops = Vec::with_capacity(global.ops.len() + route.ops.len());
        ops.extend(global.ops.iter().cloned());
        ops.extend(route.ops.iter().cloned());
        Self { ops }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// True when any template needs a generated request id, so the caller only
    /// pays for generating one when a rule actually uses it.
    pub fn needs_request_id(&self) -> bool {
        self.uses(Variable::RequestId)
    }

    /// True when any template needs the client address, which costs a
    /// `to_string` on the socket address.
    pub fn needs_client_addr(&self) -> bool {
        self.uses(Variable::ClientIp) || self.uses(Variable::ClientPort)
    }

    fn uses(&self, variable: Variable) -> bool {
        self.ops.iter().any(|op| match op {
            Op::Set(_, template) | Op::Add(_, template) => template.uses(variable),
            Op::Remove(_) => false,
        })
    }

    pub fn apply<S: HeaderSink + ?Sized>(&self, sink: &mut S, ctx: &HeaderContext<'_>) {
        for op in &self.ops {
            match op {
                Op::Remove(name) => sink.remove_header_named(name),
                Op::Set(name, template) => {
                    if let Some(value) = template.render(ctx) {
                        sink.set_header(name, value);
                    }
                }
                Op::Add(name, template) => {
                    if let Some(value) = template.render(ctx) {
                        sink.add_header(name, value);
                    }
                }
            }
        }
    }
}

/// Reports the variables a rule set uses, for diagnostics.
pub fn variables_used(rules: &HeaderRules) -> Vec<String> {
    rules
        .set
        .values()
        .chain(rules.add.values())
        .flat_map(|value| extract_variables(value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn rules(set: &[(&str, &str)], add: &[(&str, &str)], remove: &[&str]) -> HeaderRules {
        HeaderRules {
            set: set
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
            add: add
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
            remove: remove.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn ctx() -> HeaderContext<'static> {
        HeaderContext {
            client_ip: Some("203.0.113.7"),
            client_port: Some(51234),
            scheme: "http",
            host: Some("api.local"),
            route_name: Some("api"),
            upstream_addr: Some("127.0.0.1:3001"),
            request_id: Some("abc123"),
        }
    }

    #[test]
    fn set_replaces_and_add_appends() {
        let compiled = CompiledHeaderRules::compile(&rules(
            &[("x-real-ip", "$client_ip")],
            &[("x-forwarded-for", "$client_ip")],
            &[],
        ));

        let mut headers = http::HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("1.2.3.4"));
        headers.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4"));
        compiled.apply(&mut headers, &ctx());

        assert_eq!(headers.get_all("x-real-ip").iter().count(), 1);
        assert_eq!(headers.get("x-real-ip").unwrap(), "203.0.113.7");

        let forwarded: Vec<_> = headers
            .get_all("x-forwarded-for")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(forwarded, vec!["1.2.3.4", "203.0.113.7"]);
    }

    #[test]
    fn remove_runs_before_set() {
        let compiled =
            CompiledHeaderRules::compile(&rules(&[("x-token", "fresh")], &[], &["x-token"]));

        let mut headers = http::HeaderMap::new();
        headers.insert("x-token", HeaderValue::from_static("stale"));
        compiled.apply(&mut headers, &ctx());

        assert_eq!(headers.get("x-token").unwrap(), "fresh");
    }

    #[test]
    fn renders_every_variable() {
        let compiled = CompiledHeaderRules::compile(&rules(
            &[(
                "x-info",
                "$scheme://$host route=$route_name up=$upstream_addr id=$request_id from=$client_ip:$client_port",
            )],
            &[],
            &[],
        ));

        let mut headers = http::HeaderMap::new();
        compiled.apply(&mut headers, &ctx());

        assert_eq!(
            headers.get("x-info").unwrap(),
            "http://api.local route=api up=127.0.0.1:3001 id=abc123 from=203.0.113.7:51234"
        );
    }

    #[test]
    fn a_missing_variable_skips_the_header_instead_of_writing_a_hole() {
        let compiled =
            CompiledHeaderRules::compile(&rules(&[("x-upstream", "$upstream_addr")], &[], &[]));

        let mut headers = http::HeaderMap::new();
        let mut context = ctx();
        context.upstream_addr = None;
        compiled.apply(&mut headers, &context);

        assert!(headers.get("x-upstream").is_none());
    }

    #[test]
    fn literal_values_are_rendered_once_at_compile_time() {
        let compiled = CompiledHeaderRules::compile(&rules(&[("x-env", "prod")], &[], &[]));
        assert!(!compiled.needs_request_id());

        let mut headers = http::HeaderMap::new();
        compiled.apply(&mut headers, &HeaderContext::default());
        assert_eq!(headers.get("x-env").unwrap(), "prod");
    }

    #[test]
    fn request_id_use_is_detected() {
        let with_id = CompiledHeaderRules::compile(&rules(&[("x-id", "$request_id")], &[], &[]));
        let without = CompiledHeaderRules::compile(&rules(&[("x-id", "static")], &[], &[]));

        assert!(with_id.needs_request_id());
        assert!(!without.needs_request_id());
    }

    #[test]
    fn global_rules_apply_before_route_rules() {
        let global = CompiledHeaderRules::compile(&rules(&[("x-layer", "global")], &[], &[]));
        let route = CompiledHeaderRules::compile(&rules(&[("x-layer", "route")], &[], &[]));
        let merged = CompiledHeaderRules::merge(&global, &route);

        let mut headers = http::HeaderMap::new();
        merged.apply(&mut headers, &ctx());
        assert_eq!(headers.get("x-layer").unwrap(), "route");
    }
}
