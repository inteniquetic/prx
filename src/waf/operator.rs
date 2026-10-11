//! SecLang operators evaluation (T506).
//!
//! Evaluates an operator against a target value.
//! Operators can be negated (`!@...`).

#![allow(clippy::needless_range_loop)]
#![allow(clippy::collapsible_if)]

use std::collections::HashMap;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::RwLock;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use once_cell::sync::Lazy;

use super::pcre;
use super::rule::{Operator, Rx};

static PM_CACHE: Lazy<RwLock<HashMap<String, AhoCorasick>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Evaluates an operator against an input byte slice.
///
/// Returns true if the operator matches (taking `negated` into account).
/// If `captures` is provided, regex capture groups 0..9 are recorded.
pub fn evaluate_operator(
    op: &Operator,
    rx: Option<&Rx>,
    input: &[u8],
    macro_resolver: impl Fn(&str) -> Option<String>,
    captures: &mut [Option<String>; 10],
    data_dir: Option<&Path>,
) -> bool {
    let matched = eval_core(op, rx, input, &macro_resolver, captures, data_dir);
    if op.negated { !matched } else { matched }
}

fn expand_macros(text: &str, resolver: &impl Fn(&str) -> Option<String>) -> String {
    if !text.contains("%{") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("%{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(end) = after.find('}') {
            let var = &after[..end];
            if let Some(val) = resolver(var) {
                out.push_str(&val);
            }
            rest = &after[end + 1..];
        } else {
            out.push_str("%{");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn eval_core(
    op: &Operator,
    rx: Option<&Rx>,
    input: &[u8],
    resolver: &impl Fn(&str) -> Option<String>,
    captures: &mut [Option<String>; 10],
    data_dir: Option<&Path>,
) -> bool {
    let name = op.name.as_str();

    if op.is_rx() {
        if let Some(rx) = rx {
            if let Some(caps) = rx.regex.captures(input) {
                for i in 0..10 {
                    captures[i] = caps
                        .get(i)
                        .and_then(|m| std::str::from_utf8(m.as_bytes()).ok().map(str::to_string));
                }
                return true;
            }
            return false;
        }
        // Dynamic regex pattern with macros
        let expanded = expand_macros(&op.argument, resolver);
        if let Ok(compiled) = pcre::compile(&expanded, false) {
            if let Some(caps) = compiled.regex.captures(input) {
                for i in 0..10 {
                    captures[i] = caps
                        .get(i)
                        .and_then(|m| std::str::from_utf8(m.as_bytes()).ok().map(str::to_string));
                }
                return true;
            }
        }
        return false;
    }

    match name {
        "unconditionalMatch" => true,
        "eq" => compare_num(input, &expand_macros(&op.argument, resolver), |a, b| a == b),
        "ne" => compare_num(input, &expand_macros(&op.argument, resolver), |a, b| a != b),
        "lt" => compare_num(input, &expand_macros(&op.argument, resolver), |a, b| a < b),
        "le" => compare_num(input, &expand_macros(&op.argument, resolver), |a, b| a <= b),
        "gt" => compare_num(input, &expand_macros(&op.argument, resolver), |a, b| a > b),
        "ge" => compare_num(input, &expand_macros(&op.argument, resolver), |a, b| a >= b),
        "streq" => {
            let arg = expand_macros(&op.argument, resolver);
            input == arg.as_bytes()
        }
        "contains" => {
            let arg = expand_macros(&op.argument, resolver);
            find_subslice(input, arg.as_bytes())
        }
        "beginsWith" => {
            let arg = expand_macros(&op.argument, resolver);
            input.starts_with(arg.as_bytes())
        }
        "endsWith" => {
            let arg = expand_macros(&op.argument, resolver);
            input.ends_with(arg.as_bytes())
        }
        "within" => {
            let arg = expand_macros(&op.argument, resolver);
            find_subslice(arg.as_bytes(), input)
        }
        "pm" => {
            let words: Vec<&str> = op.argument.split_whitespace().collect();
            eval_pm(&words, input)
        }
        "pmFromFile" => {
            let path_str = expand_macros(&op.argument, resolver);
            let path = resolve_data_path(&path_str, data_dir);
            if let Some(ac) = get_or_load_pm_file(&path) {
                ac.is_match(input)
            } else {
                false
            }
        }
        "validateByteRange" => validate_byte_range(input, &op.argument),
        "ipMatch" => ip_match(input, &expand_macros(&op.argument, resolver)),
        "detectSQLi" => libinjectionrs::detect_sqli(input).is_injection(),
        "detectXSS" => libinjectionrs::detect_xss(input).is_injection(),
        "validateUrlEncoding" => validate_url_encoding(input),
        "validateUtf8Encoding" => std::str::from_utf8(input).is_ok(),
        _ => false,
    }
}

fn compare_num(input: &[u8], arg_str: &str, cmp: impl Fn(i64, i64) -> bool) -> bool {
    let input_str = std::str::from_utf8(input).unwrap_or("0").trim();
    let num_a = input_str.parse::<i64>().unwrap_or(0);
    let num_b = arg_str.trim().parse::<i64>().unwrap_or(0);
    cmp(num_a, num_b)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn eval_pm(words: &[&str], input: &[u8]) -> bool {
    if words.is_empty() {
        return false;
    }
    // Aho-Corasick case-insensitive
    if let Ok(ac) = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build(words)
    {
        ac.is_match(input)
    } else {
        false
    }
}

fn resolve_data_path(name: &str, data_dir: Option<&Path>) -> PathBuf {
    let p = Path::new(name);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    if let Some(dir) = data_dir {
        let candidate = dir.join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    // Try current dir or bench/waf/crs/rules
    let fallback = Path::new("bench/waf/crs/rules").join(name);
    if fallback.exists() {
        return fallback;
    }
    p.to_path_buf()
}

fn get_or_load_pm_file(path: &Path) -> Option<AhoCorasick> {
    let key = path.display().to_string();
    {
        let cache = PM_CACHE.read().ok()?;
        if let Some(ac) = cache.get(&key) {
            return Some(ac.clone());
        }
    }
    let content = fs::read_to_string(path).ok()?;
    let words: Vec<&str> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let ac = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build(&words)
        .ok()?;
    if let Ok(mut cache) = PM_CACHE.write() {
        cache.insert(key, ac.clone());
    }
    Some(ac)
}

fn validate_byte_range(input: &[u8], range_spec: &str) -> bool {
    let mut allowed = [false; 256];
    for part in range_spec.split(',') {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            if let (Ok(s), Ok(e)) = (start.trim().parse::<u8>(), end.trim().parse::<u8>()) {
                for b in s..=e {
                    allowed[b as usize] = true;
                }
            }
        } else if let Ok(single) = part.parse::<u8>() {
            allowed[single as usize] = true;
        }
    }
    input.iter().all(|&b| allowed[b as usize])
}

fn ip_match(input: &[u8], cidr_list: &str) -> bool {
    let input_str = match std::str::from_utf8(input) {
        Ok(s) => s.trim(),
        Err(_) => return false,
    };
    let ip = match IpAddr::from_str(input_str) {
        Ok(ip) => ip,
        Err(_) => return false,
    };
    for item in cidr_list.split(',') {
        let item = item.trim();
        if let Ok(exact_ip) = IpAddr::from_str(item) {
            if exact_ip == ip {
                return true;
            }
        } else if let Some((net_str, prefix_str)) = item.split_once('/') {
            if let (Ok(net_ip), Ok(prefix)) = (IpAddr::from_str(net_str), prefix_str.parse::<u8>())
            {
                match (ip, net_ip) {
                    (IpAddr::V4(addr), IpAddr::V4(net)) => {
                        if prefix <= 32 {
                            let mask = if prefix == 0 {
                                0
                            } else {
                                !0u32 << (32 - prefix)
                            };
                            if (u32::from(addr) & mask) == (u32::from(net) & mask) {
                                return true;
                            }
                        }
                    }
                    (IpAddr::V6(addr), IpAddr::V6(net)) if prefix <= 128 => {
                        let mask = if prefix == 0 {
                            0
                        } else {
                            !0u128 << (128 - prefix)
                        };
                        if (u128::from(addr) & mask) == (u128::from(net) & mask) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    false
}

fn validate_url_encoding(input: &[u8]) -> bool {
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            if i + 2 >= input.len() {
                return false;
            }
            let is_hex = |b: u8| b.is_ascii_hexdigit();
            if !is_hex(input[i + 1]) || !is_hex(input[i + 2]) {
                return false;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_operators() {
        let captures = &mut [None, None, None, None, None, None, None, None, None, None];
        let op_eq = Operator {
            name: "eq".into(),
            negated: false,
            argument: "10".into(),
        };
        assert!(evaluate_operator(
            &op_eq,
            None,
            b"10",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op_eq,
            None,
            b"11",
            |_| None,
            captures,
            None
        ));

        let op_ge = Operator {
            name: "ge".into(),
            negated: false,
            argument: "5".into(),
        };
        assert!(evaluate_operator(
            &op_ge,
            None,
            b"5",
            |_| None,
            captures,
            None
        ));
        assert!(evaluate_operator(
            &op_ge,
            None,
            b"6",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op_ge,
            None,
            b"4",
            |_| None,
            captures,
            None
        ));

        let op_lt = Operator {
            name: "lt".into(),
            negated: false,
            argument: "5".into(),
        };
        assert!(evaluate_operator(
            &op_lt,
            None,
            b"4",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op_lt,
            None,
            b"5",
            |_| None,
            captures,
            None
        ));
    }

    #[test]
    fn test_string_operators() {
        let captures = &mut [None, None, None, None, None, None, None, None, None, None];
        let op_contains = Operator {
            name: "contains".into(),
            negated: false,
            argument: "admin".into(),
        };
        assert!(evaluate_operator(
            &op_contains,
            None,
            b"/admin/login",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op_contains,
            None,
            b"/user/login",
            |_| None,
            captures,
            None
        ));

        let op_within = Operator {
            name: "within".into(),
            negated: false,
            argument: "GET POST".into(),
        };
        assert!(evaluate_operator(
            &op_within,
            None,
            b"GET",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op_within,
            None,
            b"DELETE",
            |_| None,
            captures,
            None
        ));
    }

    #[test]
    fn test_pm_operator() {
        let captures = &mut [None, None, None, None, None, None, None, None, None, None];
        let op = Operator {
            name: "pm".into(),
            negated: false,
            argument: "select union insert".into(),
        };
        assert!(evaluate_operator(
            &op,
            None,
            b"SELECT 1 FROM foo",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op,
            None,
            b"UPDATE foo SET bar=1",
            |_| None,
            captures,
            None
        ));
    }

    #[test]
    fn test_libinjection() {
        let captures = &mut [None, None, None, None, None, None, None, None, None, None];
        let op_sqli = Operator {
            name: "detectSQLi".into(),
            negated: false,
            argument: String::new(),
        };
        assert!(evaluate_operator(
            &op_sqli,
            None,
            b"1' OR '1'='1",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op_sqli,
            None,
            b"normal text",
            |_| None,
            captures,
            None
        ));

        let op_xss = Operator {
            name: "detectXSS".into(),
            negated: false,
            argument: String::new(),
        };
        assert!(evaluate_operator(
            &op_xss,
            None,
            b"<script>alert(1)</script>",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op_xss,
            None,
            b"normal text",
            |_| None,
            captures,
            None
        ));
    }

    #[test]
    fn ipv6_prefix_boundaries_are_checked_before_shifting() {
        assert!(ip_match(b"2001:db8::1", "::/0"));
        assert!(ip_match(b"2001:db8::1", "2001:db8::/32"));
        assert!(ip_match(b"2001:db8::1", "2001:db8::1/128"));
        assert!(!ip_match(b"2001:db8::2", "2001:db8::1/128"));
        assert!(!ip_match(b"2001:db8::1", "2001:db8::1/129"));
        assert!(!ip_match(b"2001:db8::1", "2001:db8::1/255"));
        assert!(!ip_match(b"192.0.2.1", "::/0"));
    }

    #[test]
    fn test_ip_match() {
        let captures = &mut [None, None, None, None, None, None, None, None, None, None];
        let op = Operator {
            name: "ipMatch".into(),
            negated: false,
            argument: "192.168.1.0/24,10.0.0.1".into(),
        };
        assert!(evaluate_operator(
            &op,
            None,
            b"192.168.1.50",
            |_| None,
            captures,
            None
        ));
        assert!(evaluate_operator(
            &op,
            None,
            b"10.0.0.1",
            |_| None,
            captures,
            None
        ));
        assert!(!evaluate_operator(
            &op,
            None,
            b"172.16.0.1",
            |_| None,
            captures,
            None
        ));
    }
}
