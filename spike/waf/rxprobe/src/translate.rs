//! PCRE syntax that Rust's `regex` rejects, rewritten to mean the same thing.
//!
//! Every CRS pattern that fails to compile does so for one of two reasons, and
//! neither is a feature `regex` lacks — they are both places where PCRE is
//! lenient and `regex` is strict:
//!
//! * a `{` that is not a valid counted repetition. PCRE treats it as a literal
//!   brace; `regex` calls it a malformed quantifier.
//! * a redundant escape inside a character class (`[\&\|]`). PCRE ignores the
//!   backslash; `regex` rejects an escape that means nothing.
//!
//! Both rewrites are local and lossless. Nothing here can express a
//! backreference or a lookaround, and nothing needs to: no CRS rule uses one.

/// Characters that keep their meaning when escaped inside a character class.
fn meaningful_in_class(c: char) -> bool {
    matches!(
        c,
        '\\' | ']' | '[' | '^' | '-' | 'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'n' | 'r' | 't'
            | 'f' | 'v' | '0' | 'x' | 'u' | 'p' | 'P' | 'b' | 'a' | 'e'
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
    let mut digits = 0;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if digits == 0 {
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

/// Rewrites a PCRE pattern into one `regex` accepts, preserving meaning.
pub fn pcre_to_rust(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut i = 0;
    let mut in_class = false;

    while i < chars.len() {
        let c = chars[i];
        match c {
            // `\x{...}`, `\p{...}` and friends take a braced argument that
            // `regex` understands natively. Copying the whole escape keeps the
            // literal-brace rule below from mangling it — which it did, turning
            // a pattern that compiled into one that did not.
            '\\' if i + 1 < chars.len()
                && matches!(chars[i + 1], 'x' | 'u' | 'U' | 'p' | 'P' | 'N')
                && chars.get(i + 2) == Some(&'{') =>
            {
                match chars[i + 2..].iter().position(|&c| c == '}') {
                    Some(offset) => {
                        let end = i + 2 + offset + 1;
                        out.extend(&chars[i..end]);
                        i = end;
                        continue;
                    }
                    None => {
                        out.push('\\');
                        out.push(chars[i + 1]);
                        i += 2;
                        continue;
                    }
                }
            }
            '\\' if i + 1 < chars.len() => {
                let next = chars[i + 1];
                if in_class && !meaningful_in_class(next) {
                    // `[\&]` is just `[&]`. Dropping the backslash is what PCRE
                    // does; keeping it is what `regex` refuses.
                    out.push(next);
                } else {
                    out.push('\\');
                    out.push(next);
                }
                i += 2;
                continue;
            }
            '[' if !in_class => {
                in_class = true;
                out.push(c);
                // A `]` immediately after `[` or `[^` is a literal ]. Copy it
                // across so the class-tracking below does not end early.
                let mut j = i + 1;
                if j < chars.len() && chars[j] == '^' {
                    out.push('^');
                    j += 1;
                }
                if j < chars.len() && chars[j] == ']' {
                    out.push('\\');
                    out.push(']');
                    j += 1;
                }
                i = j;
                continue;
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            '{' if !in_class => {
                match counted_repetition_end(&chars, i) {
                    // A real quantifier: copy it whole, braces and all.
                    Some(end) => {
                        out.extend(&chars[i..end]);
                        i = end;
                        continue;
                    }
                    // A brace PCRE reads as a literal.
                    None => {
                        out.push('\\');
                        out.push('{');
                    }
                }
            }
            '}' if !in_class => {
                // Closing braces of real repetitions are consumed with their
                // opener above; anything reaching here is literal.
                out.push('\\');
                out.push('}');
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}
