//! Variable extraction for WAF (T506).
//!
//! Provides lazy extraction of request collections (ARGS, HEADERS, COOKIES, etc.)
//! and memoization of transformation results.

#![allow(clippy::type_complexity)]
#![allow(clippy::collapsible_if)]

use crate::waf::rule::Key;
use pingora::http::RequestHeader;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

/// Holds the per-request state, including transformation cache.
pub struct Transaction<'a> {
    pub head: &'a RequestHeader,
    pub body: &'a [u8],
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
