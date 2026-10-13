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
        }
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
    let post_pairs = || -> Vec<(String, Vec<u8>)> {
        let form = head
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.to_ascii_lowercase()
                    .starts_with("application/x-www-form-urlencoded")
            });
        match (form, std::str::from_utf8(trx.body)) {
            (true, Ok(text)) => form_pairs(text),
            _ => Vec::new(),
        }
    };
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
