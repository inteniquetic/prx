//! Compiling CRS patterns with Rust's `regex` (T504, T505).
//!
//! CRS keeps its patterns inside the RE2 subset as a written policy, so no
//! rule needs a backreference or a lookaround. What is left over is the gap
//! between PCRE *as ModSecurity runs it* — on bytes, with no UTF mode — and
//! `regex`:
//!
//! * a `{` that is not a counted repetition. PCRE reads a literal brace;
//!   `regex` reads a malformed quantifier.
//! * a redundant escape inside a character class (`[\&\|]`). PCRE ignores the
//!   backslash; `regex` rejects an escape that means nothing.
//! * `\x{e2}`. To PCRE it is the byte 0xe2; to `regex` a braced escape is
//!   always the code point U+00E2, which is two other bytes.
//! * a non-ASCII literal inside a class (`[´’]`). To PCRE the class holds each
//!   of its UTF-8 bytes; `regex` has no byte class that can hold a character.
//!
//! [`repair`] rewrites all four. It is used **only when the pattern as
//! written cannot be used**: rewriting a pattern the engine already accepts
//! would be an unreviewed change to a security control.

use std::fmt::Write as _;

use regex::bytes::RegexBuilder;

use super::rule::Rx;

/// Compiles a pattern the way ModSecurity v3 and Coraza read it: `.` matches a
/// newline and `^`/`$` match at line ends (PCRE_DOTALL | PCRE_MULTILINE,
/// `(?sm)` in Coraza), and the pattern works on bytes, not code points.
///
/// Bytes are not a detail. With `regex`'s Unicode mode on, `.` and `[^x]` step
/// over whole UTF-8 sequences and stop dead at a byte that is not one, so
/// `union.*select` does not match `union\xffselect` — one stray byte would be
/// a bypass for half of CRS. It would also make `\w`, `\d`, `\s` and `\b`
/// Unicode-aware, which they are in neither PCRE nor RE2.
///
/// The error is the one for the pattern as written, not for the repair.
pub(super) fn compile(pattern: &str, case_insensitive: bool) -> Result<Rx, String> {
    let build = |pattern: &str| {
        RegexBuilder::new(pattern)
            .unicode(false)
            .dot_matches_new_line(true)
            .multi_line(true)
            .case_insensitive(case_insensitive)
            .build()
    };
    // ponytail: every @rx runs on every request. T504 measured a literal
    // prefilter (aho-corasick over literals pulled from these patterns) at 2x;
    // T508 owns it, and this is where the literals would be extracted.
    let as_written = build(pattern);
    let (rewritten, byte_escapes) = repair(pattern);
    match as_written {
        // A pattern that compiles but holds `\x{e2}` compiles to the wrong
        // bytes. That is a failure too, just a quiet one.
        Ok(regex) if !byte_escapes => Ok(Rx {
            regex,
            repaired: false,
        }),
        as_written => match (build(&rewritten), as_written) {
            (Ok(regex), _) => Ok(Rx {
                regex,
                repaired: true,
            }),
            // The last line is the reason; the ones before it echo the
            // pattern, which for CRS can run to kilobytes.
            (Err(after_repair), as_written) => Err(as_written
                .err()
                .unwrap_or(after_repair)
                .to_string()
                .lines()
                .last()
                .unwrap_or_default()
                .trim_start_matches("error: ")
                .to_string()),
        },
    }
}

/// Characters that keep their meaning when escaped inside a character class.
fn meaningful_in_class(c: char) -> bool {
    matches!(
        c,
        '\\' | ']'
            | '['
            | '^'
            | '-'
            | 'd'
            | 'D'
            | 'w'
            | 'W'
            | 's'
            | 'S'
            | 'n'
            | 'r'
            | 't'
            | 'f'
            | 'v'
            | '0'
            | 'x'
            | 'u'
            | 'p'
            | 'P'
            | 'b'
            | 'a'
            | 'e'
    )
}

/// If `{` at `start` opens a valid counted repetition — `{n}`, `{n,}` or
/// `{n,m}` — returns the index just past its closing brace.
///
/// It has to return the end, not a yes/no: the closing brace of a real
/// repetition must be copied through with its opener. Treating the two
/// independently is what turns `{1,3}` into `{1,3\}` and breaks a pattern that
/// was working.
fn counted_repetition_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let digits_from = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_from {
        return None;
    }
    if i < chars.len() && chars[i] == ',' {
        i += 1;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
    }
    (i < chars.len() && chars[i] == '}').then_some(i + 1)
}

/// Rewrites a PCRE pattern into one `regex` reads the same way. Also says
/// whether the pattern held a braced byte escape above 0x7f, which `regex`
/// accepts as written but reads as something else.
fn repair(pattern: &str) -> (String, bool) {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut byte_escapes = false;
    let mut i = 0;
    let mut in_class = false;

    while i < chars.len() {
        let c = chars[i];
        match c {
            // `\x{...}`, `\p{...}` and friends take a braced argument. Taking
            // the whole escape here keeps the literal-brace rule below from
            // mangling it.
            '\\' if matches!(chars.get(i + 1), Some('x' | 'u' | 'U' | 'p' | 'P' | 'N'))
                && chars.get(i + 2) == Some(&'{') =>
            {
                let Some(close) = chars[i + 2..].iter().position(|&c| c == '}') else {
                    out.extend(&chars[i..i + 2]);
                    i += 2;
                    continue;
                };
                let close = i + 2 + close;
                let hex: String = chars[i + 3..close].iter().collect();
                let all_hex = hex.bytes().all(|b| b.is_ascii_hexdigit());
                match u8::from_str_radix(&hex, 16) {
                    // The two-digit form is the one that means a byte.
                    Ok(byte) if chars[i + 1] == 'x' && all_hex => {
                        let _ = write!(out, "\\x{byte:02x}");
                        byte_escapes |= !byte.is_ascii();
                    }
                    _ => out.extend(&chars[i..=close]),
                }
                i = close + 1;
                continue;
            }
            '\\' if i + 1 < chars.len() => {
                let next = chars[i + 1];
                // `[\&]` is just `[&]`. Dropping the backslash is what PCRE
                // does; keeping it is what `regex` refuses.
                if !in_class || meaningful_in_class(next) {
                    out.push('\\');
                }
                out.push(next);
                i += 2;
                continue;
            }
            '[' if !in_class => {
                in_class = true;
                out.push(c);
                // A `]` immediately after `[` or `[^` is a literal ]. Copy it
                // across so the class-tracking below does not end early.
                let mut j = i + 1;
                if chars.get(j) == Some(&'^') {
                    out.push('^');
                    j += 1;
                }
                if chars.get(j) == Some(&']') {
                    out.push_str("\\]");
                    j += 1;
                }
                i = j;
                continue;
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            // `[´]` is the two bytes of `´` to PCRE, each a member.
            _ if in_class && !c.is_ascii() => {
                for byte in c.encode_utf8(&mut [0; 4]).bytes() {
                    let _ = write!(out, "\\x{byte:02x}");
                }
            }
            '{' if !in_class => match counted_repetition_end(&chars, i) {
                // A real quantifier: copy it whole, braces and all.
                Some(end) => {
                    out.extend(&chars[i..end]);
                    i = end;
                    continue;
                }
                // A brace PCRE reads as a literal.
                None => out.push_str("\\{"),
            },
            // Closing braces of real repetitions are consumed with their
            // opener above; anything reaching here is literal.
            '}' if !in_class => out.push_str("\\}"),
            _ => out.push(c),
        }
        i += 1;
    }
    (out, byte_escapes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_accepted_pattern_is_never_rewritten() {
        // `repair` would touch this one (the escaped `.`, the literal `}`),
        // which is exactly why it must not get the chance.
        let pattern = r"^[a-z\.]{1,3}}$";
        assert_ne!(repair(pattern).0, pattern);
        let rx = compile(pattern, false).unwrap();
        assert!(!rx.repaired);
        assert_eq!(rx.regex.as_str(), pattern);
    }

    #[test]
    fn pcre_leniency_is_repaired_and_marked() {
        for (pattern, hit, miss) in [
            (r"\${jndi:", &b"${jndi:ldap"[..], &b"$jndi:"[..]),
            (r"^a{,3}b{2}$", b"a{,3}bb", b"aab"),
            (r"x[^\<\&]y", b"x-y", b"x<y"),
        ] {
            let rx = compile(pattern, false).unwrap();
            assert!(rx.repaired, "{pattern}");
            assert!(rx.regex.is_match(hit), "{pattern}");
            assert!(!rx.regex.is_match(miss), "{pattern}");
        }
    }

    #[test]
    fn repair_leaves_real_quantifiers_and_braced_escapes_alone() {
        assert_eq!(repair(r"a{1,3}b{2}c{4,}").0, r"a{1,3}b{2}c{4,}");
        assert_eq!(
            repair(r"\x{41}\p{L}\x{100}\x{+5}{").0,
            r"\x41\p{L}\x{100}\x{+5}\{"
        );
        assert_eq!(repair(r"[]{}\&]{").0, r"[\]{}&]\{");
        assert_eq!(repair(r"\x{4"), (r"\x\{4".to_string(), false));
    }

    #[test]
    fn patterns_read_bytes_across_lines_like_modsecurity() {
        let rx = compile(r"union.*select$", false).unwrap();
        assert!(!rx.repaired);
        assert!(
            rx.regex.is_match(b"union\xff\nselect\nrest"),
            "a stray byte is no bypass"
        );
        assert!(
            !compile(r"^\w+$", false)
                .unwrap()
                .regex
                .is_match("é".as_bytes())
        );
        assert!(compile("^x$", true).unwrap().regex.is_match(b"X"));
    }

    #[test]
    fn braced_byte_escapes_mean_bytes() {
        // CRS v4.21.0, rule 934120 (Apache-2.0), excerpt: UTF-8 for the
        // enclosed alphanumerics (U+2460…), written byte by byte.
        let pattern = r"\x{e2}(?:\x91[\xa0-\x{bf}]|\x92[\x80-\x{bf}])";
        let rx = compile(pattern, false).unwrap();
        assert!(rx.repaired);
        assert!(rx.regex.is_match("http://①②⑦.0.0.1".as_bytes()));
        assert!(
            !rx.regex.is_match("â".as_bytes()),
            "U+00E2 is not the byte 0xe2"
        );

        // This one compiles as written, to the wrong bytes. Still repaired.
        let rx = compile(r"\x{e2}\x{91}\x{a0}", false).unwrap();
        assert!(rx.repaired);
        assert!(rx.regex.is_match("①".as_bytes()));
        // Below 0x80 both readings agree, so the pattern is left alone.
        assert!(!compile(r"\x{41}", false).unwrap().repaired);
    }

    #[test]
    fn a_character_in_a_class_is_its_bytes() {
        // CRS v4.21.0, rule 942430 (Apache-2.0), with the count cut to 2.
        let pattern = r#"((?:[~!@#\$%\^&\*\(\)\-\+=\{\}\[\]\|:;\"'´’‘`<>][^~!@#\$%\^&\*\(\)\-\+=\{\}\[\]\|:;\"'´’‘`<>]*?){2})"#;
        let rx = compile(pattern, false).unwrap();
        assert!(rx.repaired);
        assert!(
            rx.regex.is_match("a’b".as_bytes()),
            "three bytes, each one a member"
        );
        assert!(rx.regex.is_match(b"a;b;c"));
        assert!(!rx.regex.is_match(b"a;bc"));
    }

    #[test]
    fn a_pattern_beyond_repair_reports_one_line() {
        let error = compile(r"(?<=a)b", false).unwrap_err();
        assert!(!error.contains('\n'), "{error}");
        assert!(error.contains("look"), "{error}");
    }
}
