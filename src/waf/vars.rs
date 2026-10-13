//! Variable extraction for WAF (T506).
//!
//! Provides lazy extraction of request collections (ARGS, HEADERS, COOKIES, etc.)
//! and memoization of transformation results.

#![allow(clippy::type_complexity)]
#![allow(clippy::collapsible_if)]

use crate::waf::rule::Key;
use pingora::http::{RequestHeader, ResponseHeader};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

/// Holds the per-request state, including transformation cache.
pub struct Transaction<'a> {
    pub head: &'a RequestHeader,
    pub body: &'a [u8],
    /// Upstream response headers, for phase 3 and later. Empty before phase 3.
    pub response_head: Option<&'a ResponseHeader>,
    /// Response body bytes as the client receives them, for phase 4 and later.
    pub response_body: &'a [u8],
    /// Cache: (VariableCollection, SelectorKey, TransformChainIndex) -> TransformedValue
    pub transform_cache: RefCell<HashMap<(String, String, usize), Cow<'a, [u8]>>>,
    /// Anomaly Scores per phase (0-4 for phase 1-5)
    pub anomaly_scores: RefCell<[u32; 5]>,
    /// TX collection variables
    pub tx: RefCell<HashMap<String, String>>,
    /// What each distinct target selected. Request-derived collections only:
    /// TX and the response change under the rules.
    values_cache: RefCell<HashMap<String, std::rc::Rc<Vec<Extracted<'a>>>>>,
    /// Client address and port, for `REMOTE_ADDR` / `REMOTE_PORT`.
    pub remote: Option<(String, u16)>,
    /// Listener address and port, for `SERVER_ADDR` / `SERVER_PORT`.
    pub server: Option<(String, u16)>,
    /// Set by `ctl:requestBodyProcessor`; otherwise taken from Content-Type.
    pub body_processor: RefCell<Option<String>>,
    /// What the last matching link matched, as (name, value), for the
    /// `MATCHED_VAR*` collections. Rewritten on every match.
    pub matched: RefCell<Vec<(String, Vec<u8>)>>,
    parsed_body: RefCell<Option<std::rc::Rc<ParsedBody>>>,
}

/// A request body split the way its processor splits it.
#[derive(Default, Debug)]
pub struct ParsedBody {
    /// `ARGS_POST`: name and value, already decoded.
    pub args: Vec<(String, Vec<u8>)>,
    /// `FILES` / `FILES_NAMES` / `FILES_SIZES`: field, file name, size.
    pub files: Vec<(String, String, usize)>,
    /// `MULTIPART_PART_HEADERS`, one `name: value` line per header.
    pub part_headers: Vec<Vec<u8>>,
}

impl<'a> Transaction<'a> {
    pub fn new(head: &'a RequestHeader, body: &'a [u8]) -> Self {
        Self {
            head,
            body,
            response_head: None,
            response_body: b"",
            transform_cache: RefCell::new(HashMap::new()),
            anomaly_scores: RefCell::new([0; 5]),
            tx: RefCell::new(HashMap::new()),
            values_cache: RefCell::new(HashMap::new()),
            remote: None,
            server: None,
            body_processor: RefCell::new(None),
            matched: RefCell::new(Vec::new()),
            parsed_body: RefCell::new(None),
        }
    }

    /// The body processor in effect: a `ctl` override, else what Content-Type
    /// says, else none (also none for an empty body, as ModSecurity does).
    pub fn processor(&self) -> Option<String> {
        if let Some(set) = self.body_processor.borrow().clone() {
            return Some(set);
        }
        if self.body.is_empty() {
            return None;
        }
        let ct = self
            .head
            .headers
            .get("content-type")?
            .to_str()
            .ok()?
            .to_ascii_lowercase();
        if ct.starts_with("application/x-www-form-urlencoded") {
            Some("URLENCODED".into())
        } else if ct.starts_with("multipart/form-data") {
            Some("MULTIPART".into())
        } else if ct.starts_with("application/json") || ct.contains("+json") {
            Some("JSON".into())
        } else if ct.contains("xml") {
            Some("XML".into())
        } else {
            None
        }
    }

    /// Switches the body processor and forgets everything derived from it.
    pub fn set_processor(&self, name: &str) {
        *self.body_processor.borrow_mut() = Some(name.to_ascii_uppercase());
        *self.parsed_body.borrow_mut() = None;
        self.values_cache.borrow_mut().clear();
    }

    fn parsed(&self) -> std::rc::Rc<ParsedBody> {
        if let Some(hit) = self.parsed_body.borrow().as_ref() {
            return std::rc::Rc::clone(hit);
        }
        let parsed = std::rc::Rc::new(match self.processor().as_deref() {
            Some("URLENCODED") => ParsedBody {
                args: std::str::from_utf8(self.body)
                    .map(form_pairs)
                    .unwrap_or_default(),
                ..Default::default()
            },
            Some("JSON") => ParsedBody {
                args: json_args(self.body),
                ..Default::default()
            },
            Some("MULTIPART") => multipart(self.head, self.body),
            _ => ParsedBody::default(),
        });
        *self.parsed_body.borrow_mut() = Some(std::rc::Rc::clone(&parsed));
        parsed
    }

    pub fn update_score(&self, phase: usize, score: u32) {
        if phase < 5 {
            let mut scores = self.anomaly_scores.borrow_mut();
            scores[phase] += score;
        }
    }

    pub fn set_var(&self, name: &str, value: String) {
        self.tx
            .borrow_mut()
            .insert(name.to_ascii_uppercase(), value);
    }

    pub fn get_var(&self, name: &str) -> Option<String> {
        self.tx.borrow().get(&name.to_ascii_uppercase()).cloned()
    }
}

/// Mimics extraction, but caches the raw variable value.
pub fn get_variable<'a>(trx: &'a Transaction<'a>, collection: &str, key: &Key) -> Cow<'a, [u8]> {
    // If collection is TX, check the tx map first.
    if collection.eq_ignore_ascii_case("TX") {
        if let Key::Name(name) = key {
            if let Some(val) = trx.get_var(name) {
                return Cow::Owned(val.into_bytes());
            }
        }
    }

    let key_str = format!("{:?}", key);
    let cache_key = (collection.to_string(), key_str, 0); // Using 0 as a placeholder for raw

    if let Ok(cache) = trx.transform_cache.try_borrow() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let raw = extract_variable(trx, collection, key);
    if let Ok(mut cache) = trx.transform_cache.try_borrow_mut() {
        cache.insert(cache_key, raw.clone());
    }
    raw
}

/// Extracts a variable from the request, based on the selector, using raw access.
pub fn extract_variable<'a>(
    trx: &'a Transaction<'a>,
    collection: &str,
    key: &Key,
) -> Cow<'a, [u8]> {
    match collection.to_ascii_uppercase().as_str() {
        "TX" => {
            if let Key::Name(name) = key {
                if let Some(val) = trx.get_var(name) {
                    return Cow::Owned(val.into_bytes());
                }
            }
            Cow::Borrowed(b"")
        }
        "REQUEST_HEADERS" => extract_headers(trx, key),
        "REQUEST_BODY" => {
            if matches!(key, Key::All) {
                Cow::Borrowed(trx.body)
            } else {
                Cow::Borrowed(b"")
            }
        }
        "RESPONSE_HEADERS" => extract_response_headers(trx, key),
        "RESPONSE_BODY" => {
            if matches!(key, Key::All) {
                Cow::Borrowed(trx.response_body)
            } else {
                Cow::Borrowed(b"")
            }
        }
        "ARGS" | "ARGS_GET" => extract_args(trx, key, false),
        "ARGS_POST" => extract_args(trx, key, true),
        // Add more collections as needed
        _ => Cow::Borrowed(b""),
    }
}

fn extract_headers<'a>(trx: &'a Transaction<'a>, key: &Key) -> Cow<'a, [u8]> {
    match key {
        Key::Name(name) => {
            if let Some(value) = trx.head.headers.get(name) {
                Cow::Borrowed(value.as_bytes())
            } else {
                Cow::Borrowed(b"")
            }
        }
        _ => Cow::Borrowed(b""),
    }
}

fn extract_response_headers<'a>(trx: &'a Transaction<'a>, key: &Key) -> Cow<'a, [u8]> {
    match (key, trx.response_head) {
        (Key::Name(name), Some(head)) => match head.headers.get(name) {
            Some(value) => Cow::Borrowed(value.as_bytes()),
            None => Cow::Borrowed(b""),
        },
        _ => Cow::Borrowed(b""),
    }
}

fn extract_args<'a>(trx: &'a Transaction<'a>, key: &Key, post: bool) -> Cow<'a, [u8]> {
    // Basic implementation: for now, only support GET query string
    if post {
        return Cow::Borrowed(b"");
    }

    let query = trx.head.uri.query().unwrap_or("");
    match key {
        Key::Name(name) => {
            for pair in query.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    if k == name {
                        return Cow::Borrowed(v.as_bytes());
                    }
                }
            }
            Cow::Borrowed(b"")
        }
        _ => Cow::Borrowed(b""),
    }
}

/// One extracted value and the name it was found under (`ARGS:id` -> `id`).
pub type Extracted<'a> = (String, Cow<'a, [u8]>);

fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if bytes.len() >= i + 3 => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    out
}

fn form_pairs(text: &str) -> Vec<(String, Vec<u8>)> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (
                String::from_utf8_lossy(&percent_decode(name)).into_owned(),
                percent_decode(value),
            )
        })
        .collect()
}

fn selected(name: &str, key: &Key) -> bool {
    match key {
        Key::All => true,
        Key::Name(wanted) => name.eq_ignore_ascii_case(wanted),
        Key::Regex(rx) => rx.regex.is_match(name.as_bytes()),
    }
}

fn owned<'a>(pairs: Vec<(String, Vec<u8>)>, key: &Key, names: bool) -> Vec<Extracted<'a>> {
    pairs
        .into_iter()
        .filter(|(name, _)| selected(name, key))
        .map(|(name, value)| {
            let value = if names {
                name.clone().into_bytes()
            } else {
                value
            };
            (name, Cow::Owned(value))
        })
        .collect()
}

/// Every value a rule target selects, one entry per value. An empty list means
/// the variable is absent, and a rule must not run its operator on nothing:
/// ModSecurity does not, and a negated operator would otherwise fire on every
/// request that merely lacks the header or argument.
pub fn values<'a>(trx: &'a Transaction<'a>, collection: &str, key: &Key) -> Vec<Extracted<'a>> {
    let head = trx.head;
    let scalar = |value: Vec<u8>| vec![(String::new(), Cow::Owned(value))];
    let name = collection.to_ascii_uppercase();
    let post_pairs = || -> Vec<(String, Vec<u8>)> { trx.parsed().args.clone() };
    let get_pairs = || form_pairs(head.uri.query().unwrap_or(""));
    match name.as_str() {
        "ARGS" | "ARGS_NAMES" => {
            let mut all = get_pairs();
            all.extend(post_pairs());
            owned(all, key, name == "ARGS_NAMES")
        }
        "ARGS_GET" | "ARGS_GET_NAMES" => owned(get_pairs(), key, name.ends_with("_NAMES")),
        "ARGS_POST" | "ARGS_POST_NAMES" => owned(post_pairs(), key, name.ends_with("_NAMES")),
        "REQUEST_HEADERS" | "REQUEST_HEADERS_NAMES" => {
            let pairs = head
                .headers
                .iter()
                .map(|(n, v)| (n.as_str().to_string(), v.as_bytes().to_vec()))
                .collect();
            owned(pairs, key, name.ends_with("_NAMES"))
        }
        "REQUEST_COOKIES" | "REQUEST_COOKIES_NAMES" => {
            let pairs = head
                .headers
                .get_all("cookie")
                .iter()
                .filter_map(|v| v.to_str().ok())
                .flat_map(|line| line.split(';'))
                .filter_map(|c| c.trim().split_once('='))
                .map(|(n, v)| (n.to_string(), v.as_bytes().to_vec()))
                .collect();
            owned(pairs, key, name.ends_with("_NAMES"))
        }
        "RESPONSE_HEADERS" | "RESPONSE_HEADERS_NAMES" => {
            let pairs = trx
                .response_head
                .map(|h| {
                    h.headers
                        .iter()
                        .map(|(n, v)| (n.as_str().to_string(), v.as_bytes().to_vec()))
                        .collect()
                })
                .unwrap_or_default();
            owned(pairs, key, name.ends_with("_NAMES"))
        }
        "TX" => {
            let pairs = trx
                .tx
                .borrow()
                .iter()
                .map(|(n, v)| (n.clone(), v.clone().into_bytes()))
                .collect();
            owned(pairs, key, false)
        }
        "REMOTE_ADDR" => trx
            .remote
            .as_ref()
            .map(|(ip, _)| scalar(ip.clone().into_bytes()))
            .unwrap_or_default(),
        "REMOTE_PORT" => trx
            .remote
            .as_ref()
            .map(|(_, port)| scalar(port.to_string().into_bytes()))
            .unwrap_or_default(),
        "SERVER_ADDR" => trx
            .server
            .as_ref()
            .map(|(ip, _)| scalar(ip.clone().into_bytes()))
            .unwrap_or_default(),
        "SERVER_PORT" => trx
            .server
            .as_ref()
            .map(|(_, port)| scalar(port.to_string().into_bytes()))
            .unwrap_or_default(),
        "SERVER_NAME" => head
            .headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .map(|h| {
                scalar(
                    h.rsplit_once(':')
                        .filter(|(_, p)| p.parse::<u16>().is_ok())
                        .map_or(h, |(n, _)| n)
                        .as_bytes()
                        .to_vec(),
                )
            })
            .unwrap_or_default(),
        "REQBODY_PROCESSOR" => trx
            .processor()
            .map(|p| scalar(p.into_bytes()))
            .unwrap_or_default(),
        "FILES" => owned(
            trx.parsed()
                .files
                .iter()
                .map(|(field, file, _)| (field.clone(), file.clone().into_bytes()))
                .collect(),
            key,
            false,
        ),
        "FILES_NAMES" => owned(
            trx.parsed()
                .files
                .iter()
                .map(|(field, _, _)| (field.clone(), Vec::new()))
                .collect(),
            key,
            true,
        ),
        "FILES_SIZES" => owned(
            trx.parsed()
                .files
                .iter()
                .map(|(field, _, size)| (field.clone(), size.to_string().into_bytes()))
                .collect(),
            key,
            false,
        ),
        "FILES_COMBINED_SIZE" => {
            let parsed = trx.parsed();
            if parsed.files.is_empty() {
                Vec::new()
            } else {
                scalar(
                    parsed
                        .files
                        .iter()
                        .map(|(_, _, size)| size)
                        .sum::<usize>()
                        .to_string()
                        .into_bytes(),
                )
            }
        }
        "MULTIPART_PART_HEADERS" => trx
            .parsed()
            .part_headers
            .iter()
            .map(|line| {
                let name = line
                    .iter()
                    .position(|&b| b == b':')
                    .map(|i| String::from_utf8_lossy(&line[..i]).to_ascii_lowercase())
                    .unwrap_or_default();
                (name, Cow::Owned(line.clone()))
            })
            .filter(|(name, _)| selected(name, key))
            .collect(),
        "MATCHED_VAR" => trx
            .matched
            .borrow()
            .last()
            .map(|(_, v)| scalar(v.clone()))
            .unwrap_or_default(),
        "MATCHED_VAR_NAME" => trx
            .matched
            .borrow()
            .last()
            .map(|(n, _)| scalar(n.clone().into_bytes()))
            .unwrap_or_default(),
        "MATCHED_VARS" => trx
            .matched
            .borrow()
            .iter()
            .map(|(n, v)| (n.clone(), Cow::Owned(v.clone())))
            .filter(|(n, _)| selected(n, key))
            .collect(),
        "MATCHED_VARS_NAMES" => trx
            .matched
            .borrow()
            .iter()
            .map(|(n, _)| (n.clone(), Cow::Owned(n.clone().into_bytes())))
            .filter(|(n, _)| selected(n, key))
            .collect(),
        "REQUEST_METHOD" => scalar(head.method.as_str().as_bytes().to_vec()),
        "REQUEST_PROTOCOL" => scalar(format!("{:?}", head.version).into_bytes()),
        "REQUEST_URI" | "REQUEST_URI_RAW" => scalar(
            head.uri
                .path_and_query()
                .map(|pq| pq.as_str())
                .unwrap_or("/")
                .as_bytes()
                .to_vec(),
        ),
        "REQUEST_LINE" => {
            let uri = head
                .uri
                .path_and_query()
                .map(|pq| pq.as_str())
                .unwrap_or("/");
            scalar(format!("{} {} {:?}", head.method, uri, head.version).into_bytes())
        }
        "REQUEST_FILENAME" | "PATH_INFO" => scalar(head.uri.path().as_bytes().to_vec()),
        "REQUEST_BASENAME" => scalar(
            head.uri
                .path()
                .rsplit('/')
                .next()
                .unwrap_or("")
                .as_bytes()
                .to_vec(),
        ),
        "QUERY_STRING" => match head.uri.query() {
            Some(query) => scalar(query.as_bytes().to_vec()),
            None => Vec::new(),
        },
        "REQUEST_BODY" if matches!(key, Key::All) && !trx.body.is_empty() => {
            vec![(String::new(), Cow::Borrowed(trx.body))]
        }
        "REQUEST_BODY_LENGTH" if !trx.body.is_empty() => {
            scalar(trx.body.len().to_string().into_bytes())
        }
        "RESPONSE_BODY" if matches!(key, Key::All) && !trx.response_body.is_empty() => {
            vec![(String::new(), Cow::Borrowed(trx.response_body))]
        }
        _ => Vec::new(),
    }
}

/// [`values`] for a rule target, remembered for the rest of the transaction so
/// that `ARGS` is parsed and decoded once, not by every rule that names it.
/// Keyed by what is selected, not by which rule asked: hundreds of CRS rules
/// carry their own `ARGS` target.
pub fn values_cached<'a>(
    trx: &'a Transaction<'a>,
    target: &crate::waf::rule::Target,
) -> std::rc::Rc<Vec<Extracted<'a>>> {
    use std::rc::Rc;
    // TX changes under the rules and the response is not there yet in phase 1.
    let volatile = target.collection.eq_ignore_ascii_case("TX")
        || target
            .collection
            .to_ascii_uppercase()
            .starts_with("MATCHED_VAR")
        || target
            .collection
            .get(..8)
            .is_some_and(|c| c.eq_ignore_ascii_case("RESPONSE"));
    if volatile {
        return Rc::new(values(trx, &target.collection, &target.key));
    }
    if let Some(hit) = trx.values_cache.borrow().get(target.select_id.as_str()) {
        return Rc::clone(hit);
    }
    let all = {
        let cached = trx
            .values_cache
            .borrow()
            .get(target.collection_id.as_str())
            .cloned();
        match cached {
            Some(all) => all,
            None => {
                let all = Rc::new(values(trx, &target.collection, &Key::All));
                trx.values_cache
                    .borrow_mut()
                    .insert(target.collection_id.clone(), Rc::clone(&all));
                all
            }
        }
    };
    let found = match &target.key {
        Key::All => all,
        // Scalars ignore the selector; everything else filters the full set.
        key => Rc::new(
            all.iter()
                .filter(|(n, _)| n.is_empty() || selected(n, key))
                .cloned()
                .collect(),
        ),
    };
    trx.values_cache
        .borrow_mut()
        .insert(target.select_id.clone(), Rc::clone(&found));
    found
}

/// `json.a.b` / `json.list.0` names, the way ModSecurity's JSON processor
/// flattens a document into ARGS. Bounded so a deep or huge body is not a
/// way to make the WAF spend unbounded time.
fn json_args(body: &[u8]) -> Vec<(String, Vec<u8>)> {
    fn walk(
        prefix: &str,
        value: &serde_json::Value,
        depth: usize,
        out: &mut Vec<(String, Vec<u8>)>,
    ) {
        const MAX_DEPTH: usize = 64;
        const MAX_ARGS: usize = 10_000;
        if depth > MAX_DEPTH || out.len() >= MAX_ARGS {
            return;
        }
        match value {
            serde_json::Value::Object(map) => {
                for (key, inner) in map {
                    walk(&format!("{prefix}.{key}"), inner, depth + 1, out);
                }
            }
            serde_json::Value::Array(items) => {
                for (i, inner) in items.iter().enumerate() {
                    walk(&format!("{prefix}.{i}"), inner, depth + 1, out);
                }
            }
            serde_json::Value::String(text) => {
                out.push((prefix.to_string(), text.clone().into_bytes()))
            }
            serde_json::Value::Null => out.push((prefix.to_string(), Vec::new())),
            other => out.push((prefix.to_string(), other.to_string().into_bytes())),
        }
    }
    let mut out = Vec::new();
    if let Ok(doc) = serde_json::from_slice::<serde_json::Value>(body) {
        walk("json", &doc, 0, &mut out);
    }
    out
}

fn multipart(head: &RequestHeader, body: &[u8]) -> ParsedBody {
    let mut parsed = ParsedBody::default();
    let boundary = head
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(|ct| {
            ct.split(';')
                .map(str::trim)
                .find_map(|p| p.strip_prefix("boundary="))
        })
        .map(|b| b.trim_matches('"').as_bytes().to_vec());
    let Some(boundary) = boundary.filter(|b| !b.is_empty()) else {
        return parsed;
    };
    let delimiter = [b"--".as_slice(), &boundary].concat();
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    let mut rest = body;
    // Skip the preamble, then take each part up to the next delimiter.
    let Some(start) = find(rest, &delimiter) else {
        return parsed;
    };
    rest = &rest[start + delimiter.len()..];
    while !rest.starts_with(b"--") && !rest.is_empty() {
        let part_end = find(rest, &delimiter).unwrap_or(rest.len());
        let part = rest[..part_end]
            .strip_prefix(b"\r\n")
            .unwrap_or(&rest[..part_end]);
        if let Some(split) = find(part, b"\r\n\r\n") {
            let (raw_headers, data) = (&part[..split], &part[split + 4..]);
            let data = data.strip_suffix(b"\r\n").unwrap_or(data);
            let mut field = String::new();
            let mut file = None;
            for line in raw_headers.split(|&b| b == b'\n') {
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                if line.is_empty() {
                    continue;
                }
                parsed.part_headers.push(line.to_vec());
                let text = String::from_utf8_lossy(line);
                if text
                    .to_ascii_lowercase()
                    .starts_with("content-disposition:")
                {
                    for piece in text.split(';').map(str::trim) {
                        if let Some(v) = piece.strip_prefix("name=") {
                            field = v.trim_matches('"').to_string();
                        } else if let Some(v) = piece.strip_prefix("filename=") {
                            file = Some(v.trim_matches('"').to_string());
                        }
                    }
                }
            }
            match file {
                Some(name) => parsed.files.push((field, name, data.len())),
                None => parsed.args.push((field, data.to_vec())),
            }
        }
        if part_end >= rest.len() {
            break;
        }
        rest = &rest[part_end + delimiter.len()..];
    }
    parsed
}
