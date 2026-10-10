//! Variable extraction for WAF (T506).
//!
//! Provides lazy extraction of request collections (ARGS, HEADERS, COOKIES, etc.)
//! and memoization of transformation results.

use std::borrow::Cow;
use std::collections::HashMap;
use std::cell::RefCell;
use crate::waf::rule::Key;
use pingora::http::RequestHeader;

/// Holds the per-request state, including transformation cache.
pub struct Transaction<'a> {
    pub head: &'a RequestHeader,
    pub body: &'a [u8],
    /// Cache: (VariableCollection, SelectorKey, TransformChainIndex) -> TransformedValue
    pub transform_cache: RefCell<HashMap<(String, String, usize), Cow<'a, [u8]>>>,
}

impl<'a> Transaction<'a> {
    pub fn new(head: &'a RequestHeader, body: &'a [u8]) -> Self {
        Self {
            head,
            body,
            transform_cache: RefCell::new(HashMap::new()),
        }
    }
}

/// Mimics extraction, but caches the raw variable value.
pub fn get_variable<'a>(
    trx: &'a Transaction<'a>,
    collection: &str,
    key: &Key,
) -> Cow<'a, [u8]> {
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
    if post { return Cow::Borrowed(b""); }
    
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
