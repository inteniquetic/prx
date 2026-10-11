//! Transformations for SecLang / WAF targets (T506).
//!
//! A transformation takes bytes in and produces bytes out (as `Cow<[u8]>`).
//! Transformations can be chained: `t:lowercase,t:urlDecodeUni`.
//! In SecLang, multiple transformations run in the order declared.

#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_character_iteration)]

use std::borrow::Cow;

/// Applies a single named transformation to input bytes.
pub fn transform<'a>(name: &str, input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    match name.to_ascii_lowercase().as_str() {
        "none" => input,
        "lowercase" => transform_lowercase(input),
        "urldecode" => transform_url_decode(input, false),
        "urldecodeuni" => transform_url_decode(input, true),
        "htmlentitydecode" => transform_html_entity_decode(input),
        "removewhitespace" => transform_remove_whitespace(input),
        "compresswhitespace" => transform_compress_whitespace(input),
        "removenulls" => transform_remove_nulls(input),
        "replacecomments" => transform_replace_comments(input),
        "removecommentschar" => transform_remove_comments_char(input),
        "normalizepath" | "normalisepath" => transform_normalize_path(input, false),
        "normalizepathwin" => transform_normalize_path(input, true),
        "base64decode" => transform_base64_decode(input),
        "hexdecode" => transform_hex_decode(input),
        "hexencode" => transform_hex_encode(input),
        "length" => transform_length(input),
        "utf8tounicode" => transform_utf8_to_unicode(input),
        "cmdline" => transform_cmd_line(input),
        "jsdecode" => transform_js_decode(input),
        "cssdecode" => transform_css_decode(input),
        _ => input,
    }
}

/// Applies a chain of transformations in sequence.
pub fn apply_chain<'a>(chain: &[String], mut input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    for name in chain {
        input = transform(name, input);
    }
    input
}

fn transform_lowercase<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    if input.iter().any(|&b| b.is_ascii_uppercase()) {
        Cow::Owned(input.to_ascii_lowercase())
    } else {
        input
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn transform_url_decode<'a>(input: Cow<'a, [u8]>, unicode: bool) -> Cow<'a, [u8]> {
    if !input.contains(&b'%') && !input.contains(&b'+') {
        return input;
    }
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    let s = input.as_ref();
    let len = s.len();

    while i < len {
        if s[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if s[i] == b'%' && i + 2 < len {
            if unicode && (s[i + 1] == b'u' || s[i + 1] == b'U') && i + 5 < len {
                // %uXXXX format
                if let (Some(h1), Some(h2), Some(h3), Some(h4)) = (
                    hex_val(s[i + 2]),
                    hex_val(s[i + 3]),
                    hex_val(s[i + 4]),
                    hex_val(s[i + 5]),
                ) {
                    let cp =
                        ((h1 as u32) << 12) | ((h2 as u32) << 8) | ((h3 as u32) << 4) | (h4 as u32);
                    if let Some(ch) = char::from_u32(cp) {
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        i += 6;
                        continue;
                    }
                }
            }
            if let (Some(h1), Some(h2)) = (hex_val(s[i + 1]), hex_val(s[i + 2])) {
                out.push((h1 << 4) | h2);
                i += 3;
            } else {
                out.push(s[i]);
                i += 1;
            }
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    Cow::Owned(out)
}

fn transform_html_entity_decode<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    if !input.contains(&b'&') {
        return input;
    }
    let mut out = Vec::with_capacity(input.len());
    let s = input.as_ref();
    let len = s.len();
    let mut i = 0;

    while i < len {
        if s[i] == b'&' {
            let rest = &s[i + 1..];
            if let Some(semi) = rest.iter().position(|&b| b == b';') {
                let entity = &rest[..semi];
                let mut matched = false;
                if entity.starts_with(b"#x") || entity.starts_with(b"#X") {
                    if let Ok(entity_str) = std::str::from_utf8(&entity[2..]) {
                        if let Ok(cp) = u32::from_str_radix(entity_str, 16) {
                            if let Some(ch) = char::from_u32(cp) {
                                let mut buf = [0u8; 4];
                                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                                i += 1 + semi + 1;
                                matched = true;
                            }
                        }
                    }
                } else if entity.starts_with(b"#") {
                    if let Ok(entity_str) = std::str::from_utf8(&entity[1..]) {
                        if let Ok(cp) = entity_str.parse::<u32>() {
                            if let Some(ch) = char::from_u32(cp) {
                                let mut buf = [0u8; 4];
                                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                                i += 1 + semi + 1;
                                matched = true;
                            }
                        }
                    }
                } else {
                    let decoded_byte: Option<&[u8]> = match entity {
                        b"quot" => Some(b"\""),
                        b"amp" => Some(b"&"),
                        b"lt" => Some(b"<"),
                        b"gt" => Some(b">"),
                        b"apos" => Some(b"'"),
                        b"nbsp" => Some(b" "),
                        _ => None,
                    };
                    if let Some(bytes) = decoded_byte {
                        out.extend_from_slice(bytes);
                        i += 1 + semi + 1;
                        matched = true;
                    }
                }
                if matched {
                    continue;
                }
            }
        }
        out.push(s[i]);
        i += 1;
    }
    Cow::Owned(out)
}

fn transform_remove_whitespace<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    let is_ws = |b: u8| matches!(b, b' ' | 9 | b'\r' | b'\n' | 0x0b | 0x0c);
    if input.iter().any(|&b| is_ws(b)) {
        Cow::Owned(input.iter().copied().filter(|&b| !is_ws(b)).collect())
    } else {
        input
    }
}

fn transform_compress_whitespace<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    let is_ws = |b: u8| matches!(b, b' ' | 9 | b'\r' | b'\n' | 0x0b | 0x0c);
    let mut out = Vec::with_capacity(input.len());
    let mut in_ws = false;
    for &b in input.as_ref() {
        if is_ws(b) {
            if !in_ws {
                out.push(b' ');
                in_ws = true;
            }
        } else {
            out.push(b);
            in_ws = false;
        }
    }
    Cow::Owned(out)
}

fn transform_remove_nulls<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    if input.contains(&0) {
        Cow::Owned(input.iter().copied().filter(|&b| b != 0).collect())
    } else {
        input
    }
}

fn transform_replace_comments<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    let s = input.as_ref();
    if !s.windows(2).any(|w| w == b"/*") {
        return input;
    }
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    let len = s.len();
    while i < len {
        if i + 1 < len && s[i] == b'/' && s[i + 1] == b'*' {
            let mut j = i + 2;
            let mut closed = false;
            while j + 1 < len {
                if s[j] == b'*' && s[j + 1] == b'/' {
                    closed = true;
                    i = j + 2;
                    break;
                }
                j += 1;
            }
            out.push(b' ');
            if !closed {
                break;
            }
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    Cow::Owned(out)
}

fn transform_remove_comments_char<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    let is_comment_char = |b: u8| matches!(b, b'/' | b'*' | b'#' | b'-');
    if input.iter().any(|&b| is_comment_char(b)) {
        Cow::Owned(
            input
                .iter()
                .copied()
                .filter(|&b| !is_comment_char(b))
                .collect(),
        )
    } else {
        input
    }
}

fn transform_normalize_path<'a>(input: Cow<'a, [u8]>, win: bool) -> Cow<'a, [u8]> {
    let mut s = input.to_vec();
    if win {
        for b in &mut s {
            if *b == b'\\' {
                *b = b'/';
            }
        }
    }
    let mut parts: Vec<&[u8]> = Vec::new();
    let is_absolute = s.starts_with(b"/");
    for seg in s.split(|&b| b == b'/') {
        if seg.is_empty() || seg == b"." {
            continue;
        }
        if seg == b".." {
            if !parts.is_empty() && parts.last() != Some(&b"..".as_slice()) {
                parts.pop();
            } else if !is_absolute {
                parts.push(seg);
            }
        } else {
            parts.push(seg);
        }
    }
    let mut out = Vec::with_capacity(s.len());
    if is_absolute {
        out.push(b'/');
    }
    for (idx, seg) in parts.iter().enumerate() {
        if idx > 0 {
            out.push(b'/');
        }
        out.extend_from_slice(seg);
    }
    if s.ends_with(b"/") && !out.ends_with(b"/") {
        out.push(b'/');
    }
    Cow::Owned(out)
}

fn transform_base64_decode<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    let b64_val = |b: u8| match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let clean: Vec<u8> = input
        .iter()
        .copied()
        .filter(|&b| b64_val(b).is_some() || b == b'=')
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < clean.len() {
        let (c0, c1, c2, c3) = (clean[i], clean[i + 1], clean[i + 2], clean[i + 3]);
        let v0 = b64_val(c0).unwrap_or(0);
        let v1 = b64_val(c1).unwrap_or(0);
        out.push((v0 << 2) | (v1 >> 4));
        if c2 != b'=' {
            let v2 = b64_val(c2).unwrap_or(0);
            out.push(((v1 & 0x0f) << 4) | (v2 >> 2));
            if c3 != b'=' {
                let v3 = b64_val(c3).unwrap_or(0);
                out.push(((v2 & 0x03) << 6) | v3);
            }
        }
        i += 4;
    }
    Cow::Owned(out)
}

fn transform_hex_decode<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    let s = input.as_ref();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i + 1 < s.len() {
        if let (Some(h1), Some(h2)) = (hex_val(s[i]), hex_val(s[i + 1])) {
            out.push((h1 << 4) | h2);
            i += 2;
        } else {
            break;
        }
    }
    Cow::Owned(out)
}

fn transform_hex_encode<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(input.len() * 2);
    for &b in input.as_ref() {
        out.push(HEX[(b >> 4) as usize]);
        out.push(HEX[(b & 0x0f) as usize]);
    }
    Cow::Owned(out)
}

fn transform_length<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    Cow::Owned(input.len().to_string().into_bytes())
}

fn transform_utf8_to_unicode<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    // In ModSecurity, utf8toUnicode converts UTF-8 multi-byte sequences into %uXXXX
    let s = input.as_ref();
    if let Ok(text) = std::str::from_utf8(s) {
        if text.chars().all(|c| c.is_ascii()) {
            return input;
        }
        let mut out = Vec::new();
        for ch in text.chars() {
            if ch.is_ascii() {
                out.push(ch as u8);
            } else {
                let hex = format!("%u{:04x}", ch as u32);
                out.extend_from_slice(hex.as_bytes());
            }
        }
        Cow::Owned(out)
    } else {
        input
    }
}

fn transform_cmd_line<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    let mut out = Vec::with_capacity(input.len());
    let mut in_ws = false;
    for &b in input.as_ref() {
        match b {
            b'\\' | b'"' | b'\'' | b'^' => {}
            b' ' | 9 | b'\r' | b'\n' | 0x0b | 0x0c | b',' | b';' => {
                if !in_ws {
                    out.push(b' ');
                    in_ws = true;
                }
            }
            other => {
                out.push(other.to_ascii_lowercase());
                in_ws = false;
            }
        }
    }
    Cow::Owned(out)
}

fn transform_js_decode<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    if !input.contains(&b'\\') {
        return input;
    }
    let s = input.as_ref();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' && i + 1 < s.len() {
            match s[i + 1] {
                b'n' => {
                    out.push(b'\n');
                    i += 2;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 2;
                }
                b't' => {
                    out.push(9);
                    i += 2;
                }
                b'x' if i + 3 < s.len() => {
                    if let (Some(h1), Some(h2)) = (hex_val(s[i + 2]), hex_val(s[i + 3])) {
                        out.push((h1 << 4) | h2);
                        i += 4;
                    } else {
                        out.push(s[i]);
                        i += 1;
                    }
                }
                b'u' if i + 5 < s.len() => {
                    if let (Some(h1), Some(h2), Some(h3), Some(h4)) = (
                        hex_val(s[i + 2]),
                        hex_val(s[i + 3]),
                        hex_val(s[i + 4]),
                        hex_val(s[i + 5]),
                    ) {
                        let cp = ((h1 as u32) << 12)
                            | ((h2 as u32) << 8)
                            | ((h3 as u32) << 4)
                            | (h4 as u32);
                        if let Some(ch) = char::from_u32(cp) {
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                            i += 6;
                            continue;
                        }
                    }
                    out.push(s[i]);
                    i += 1;
                }
                other => {
                    out.push(other);
                    i += 2;
                }
            }
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    Cow::Owned(out)
}

fn transform_css_decode<'a>(input: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
    if !input.contains(&b'\\') {
        return input;
    }
    let s = input.as_ref();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' && i + 1 < s.len() {
            let mut j = i + 1;
            let mut hex_digits = 0;
            let mut val = 0u32;
            while j < s.len() && hex_digits < 6 {
                if let Some(h) = hex_val(s[j]) {
                    val = (val << 4) | (h as u32);
                    hex_digits += 1;
                    j += 1;
                } else {
                    break;
                }
            }
            if hex_digits > 0 {
                if let Some(ch) = char::from_u32(val) {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    if j < s.len() && s[j] == b' ' {
                        j += 1; // CSS escape allows optional space delimiter
                    }
                    i = j;
                    continue;
                }
            }
            out.push(s[i + 1]);
            i += 2;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lowercase() {
        assert_eq!(
            transform("lowercase", Cow::Borrowed(b"HeLLo World")).as_ref(),
            b"hello world"
        );
    }

    #[test]
    fn test_urldecode() {
        assert_eq!(
            transform("urlDecode", Cow::Borrowed(b"hello+world%21")).as_ref(),
            b"hello world!"
        );
        assert_eq!(
            transform("urlDecodeUni", Cow::Borrowed(b"test%u0020uni")).as_ref(),
            b"test uni"
        );
    }

    #[test]
    fn test_html_entity_decode() {
        assert_eq!(
            transform(
                "htmlEntityDecode",
                Cow::Borrowed(b"&lt;script&gt;&amp;&#x41;&#65;")
            )
            .as_ref(),
            b"<script>&AA"
        );
    }

    #[test]
    fn test_whitespace() {
        assert_eq!(
            transform("removeWhitespace", Cow::Borrowed(b" a 	 b \n c ")).as_ref(),
            b"abc"
        );
        assert_eq!(
            transform("compressWhitespace", Cow::Borrowed(b" a 	\n b  c ")).as_ref(),
            b" a b c "
        );
    }

    #[test]
    fn test_replace_comments() {
        assert_eq!(
            transform("replaceComments", Cow::Borrowed(b"select/*comment*/1")).as_ref(),
            b"select 1"
        );
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            transform("normalizePath", Cow::Borrowed(b"/a/b/../c/./d")).as_ref(),
            b"/a/c/d"
        );
        assert_eq!(
            transform("normalizePathWin", Cow::Borrowed(b"\\a\\b\\..\\c")).as_ref(),
            b"/a/c"
        );
    }

    #[test]
    fn test_cmdline() {
        assert_eq!(
            transform("cmdLine", Cow::Borrowed(b"c^a^t,/etc/passwd")).as_ref(),
            b"cat /etc/passwd"
        );
    }

    #[test]
    fn test_base64_and_hex() {
        assert_eq!(
            transform("base64Decode", Cow::Borrowed(b"aGVsbG8=")).as_ref(),
            b"hello"
        );
        assert_eq!(
            transform("hexDecode", Cow::Borrowed(b"414243")).as_ref(),
            b"ABC"
        );
    }
}
