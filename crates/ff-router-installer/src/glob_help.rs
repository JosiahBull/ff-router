//! Explain and highlight glob patterns as the user types them, so it is clear
//! which characters are wildcards (`*`, `?`, `[...]`, `{a,b}`) and what the
//! whole pattern will actually match.
//!
//! The semantics mirror how the router matches URLs (`ff-router`'s config uses
//! `globset` with `literal_separator(false)` and `backslash_escape(true)`):
//! matches are anchored to the *entire* URL, `*`/`?` cross `/`, and `\` escapes
//! the following metacharacter into a literal.

/// What a slice of a pattern does when matching.
#[derive(Debug, PartialEq, Eq)]
pub enum Kind {
    /// Ordinary text that matches itself.
    Literal,
    /// `*` — any run of characters (including `/`).
    Star,
    /// `?` — exactly one character.
    Any,
    /// `\x` — the following metacharacter matched as a literal.
    Escaped,
    /// `[...]` — one character from (or, when negated, not from) a set.
    Class { negated: bool, body: String },
    /// `{a,b,c}` — any one of the comma-separated alternatives.
    Alt { parts: Vec<String> },
}

/// A classified slice of a pattern: its raw source `text` and what it `kind` does.
#[derive(Debug, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub kind: Kind,
}

/// Split `pattern` into classified tokens. Unterminated `[`/`{` are treated as
/// literal characters (as globset would reject them, but here we just describe).
pub fn tokenize(pattern: &str) -> Vec<Token> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' if i + 1 < chars.len() => {
                flush(&mut lit, &mut out);
                out.push(Token {
                    text: chars[i..i + 2].iter().collect(),
                    kind: Kind::Escaped,
                });
                i += 2;
            }
            '*' => {
                flush(&mut lit, &mut out);
                out.push(Token {
                    text: "*".into(),
                    kind: Kind::Star,
                });
                i += 1;
            }
            '?' => {
                flush(&mut lit, &mut out);
                out.push(Token {
                    text: "?".into(),
                    kind: Kind::Any,
                });
                i += 1;
            }
            '[' => match scan_class(&chars, i) {
                Some((end, negated, body)) => {
                    flush(&mut lit, &mut out);
                    out.push(Token {
                        text: chars[i..=end].iter().collect(),
                        kind: Kind::Class { negated, body },
                    });
                    i = end + 1;
                }
                None => {
                    lit.push('[');
                    i += 1;
                }
            },
            '{' => match scan_alt(&chars, i) {
                Some((end, parts)) => {
                    flush(&mut lit, &mut out);
                    out.push(Token {
                        text: chars[i..=end].iter().collect(),
                        kind: Kind::Alt { parts },
                    });
                    i = end + 1;
                }
                None => {
                    lit.push('{');
                    i += 1;
                }
            },
            c => {
                lit.push(c);
                i += 1;
            }
        }
    }
    flush(&mut lit, &mut out);
    out
}

/// A plain-English description of what `pattern` matches. Framed around the
/// whole-URL anchoring so the effect of a missing leading/trailing `*` is clear.
pub fn describe(pattern: &str) -> String {
    let tokens = tokenize(pattern);
    match tokens.as_slice() {
        [] => "matches an empty URL".into(),
        [only] if only.kind == Kind::Literal => {
            format!("matches only the exact URL \u{201c}{}\u{201d}", only.text)
        }
        _ => {
            let parts: Vec<String> = tokens.iter().map(phrase).collect();
            format!("matches any URL made of: {}", parts.join(", then "))
        }
    }
}

/// Describe a single token in prose.
fn phrase(tok: &Token) -> String {
    match &tok.kind {
        Kind::Literal => format!("\u{201c}{}\u{201d}", tok.text),
        Kind::Star => "any text".into(),
        Kind::Any => "any single character".into(),
        Kind::Escaped => {
            let ch = tok.text.chars().nth(1).unwrap_or('?');
            format!("a literal \u{201c}{ch}\u{201d}")
        }
        Kind::Class { negated, body } => format!(
            "one character {}in the set [{}]",
            if *negated { "not " } else { "" },
            body
        ),
        Kind::Alt { parts } => {
            let quoted: Vec<String> = parts
                .iter()
                .map(|p| format!("\u{201c}{p}\u{201d}"))
                .collect();
            format!("either {}", join_or(&quoted))
        }
    }
}

/// Join items with commas and a trailing "or": `a`, `a or b`, `a, b, or c`.
fn join_or(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} or {b}"),
        [rest @ .., last] => format!("{}, or {}", rest.join(", "), last),
    }
}

/// Scan a `[...]` class starting at `start` (which is `[`). Returns the index of
/// the closing `]`, whether it is negated, and the inner body (sans brackets and
/// the negation marker). A `]` immediately after `[` / `[!` / `[^` is a literal
/// member, not the close.
fn scan_class(chars: &[char], start: usize) -> Option<(usize, bool, String)> {
    let mut j = start + 1;
    let negated = matches!(chars.get(j), Some('!' | '^'));
    if negated {
        j += 1;
    }
    let body_start = j;
    if chars.get(j) == Some(&']') {
        j += 1;
    }
    while j < chars.len() && chars[j] != ']' {
        j += 1;
    }
    (chars.get(j) == Some(&']')).then(|| (j, negated, chars[body_start..j].iter().collect()))
}

/// Scan a `{a,b}` alternation starting at `start` (which is `{`). Returns the
/// index of the closing `}` and the comma-separated alternatives.
fn scan_alt(chars: &[char], start: usize) -> Option<(usize, Vec<String>)> {
    let mut j = start + 1;
    while j < chars.len() && chars[j] != '}' {
        j += 1;
    }
    (chars.get(j) == Some(&'}')).then(|| {
        let body: String = chars[start + 1..j].iter().collect();
        (j, body.split(',').map(str::to_string).collect())
    })
}

fn flush(lit: &mut String, out: &mut Vec<Token>) {
    if !lit.is_empty() {
        out.push(Token {
            text: std::mem::take(lit),
            kind: Kind::Literal,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(pattern: &str) -> Vec<Kind> {
        tokenize(pattern).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn tokenizes_star_and_question() {
        assert_eq!(
            kinds("*a?b"),
            vec![Kind::Star, Kind::Literal, Kind::Any, Kind::Literal]
        );
        let toks = tokenize("*a?b");
        assert_eq!(toks[1].text, "a");
        assert_eq!(toks[3].text, "b");
    }

    #[test]
    fn tokenizes_class_with_negation_and_range() {
        let toks = tokenize("x[!a-z]y[abc]");
        assert_eq!(toks[0].text, "x");
        assert_eq!(
            toks[1].kind,
            Kind::Class {
                negated: true,
                body: "a-z".into()
            }
        );
        assert_eq!(toks[1].text, "[!a-z]");
        assert_eq!(
            toks[3].kind,
            Kind::Class {
                negated: false,
                body: "abc".into()
            }
        );
    }

    #[test]
    fn tokenizes_alternation() {
        let toks = tokenize("{com,net,org}");
        assert_eq!(
            toks[0].kind,
            Kind::Alt {
                parts: vec!["com".into(), "net".into(), "org".into()]
            }
        );
        assert_eq!(toks[0].text, "{com,net,org}");
    }

    #[test]
    fn escaped_metacharacter_is_literal() {
        let toks = tokenize(r"a\*b");
        assert_eq!(toks[1].kind, Kind::Escaped);
        assert_eq!(toks[1].text, r"\*");
    }

    #[test]
    fn unterminated_class_and_alt_are_literal() {
        // A lone `[` or `{` with no close is just text.
        assert_eq!(kinds("a[b"), vec![Kind::Literal]);
        assert_eq!(tokenize("a[b")[0].text, "a[b");
        assert_eq!(kinds("a{b"), vec![Kind::Literal]);
    }

    #[test]
    fn describes_exact_url_for_bare_pattern() {
        assert_eq!(
            describe("github.com"),
            "matches only the exact URL \u{201c}github.com\u{201d}"
        );
    }

    #[test]
    fn describes_wildcards() {
        assert_eq!(
            describe("*partly.com/*"),
            "matches any URL made of: any text, then \u{201c}partly.com/\u{201d}, then any text"
        );
        assert!(describe("*://*.atlassian.net/*").contains("\u{201c}://\u{201d}"));
        assert!(describe("a?b").contains("any single character"));
    }

    #[test]
    fn describes_class_and_alternation() {
        assert!(describe("[!a-z]").contains("one character not in the set [a-z]"));
        assert!(describe("{a,b}").contains("either \u{201c}a\u{201d} or \u{201c}b\u{201d}"));
        assert!(
            describe("{a,b,c}")
                .contains("\u{201c}a\u{201d}, \u{201c}b\u{201d}, or \u{201c}c\u{201d}")
        );
    }

    #[test]
    fn escaped_star_is_described_as_literal() {
        assert!(describe(r"a\*").contains("a literal \u{201c}*\u{201d}"));
    }
}
