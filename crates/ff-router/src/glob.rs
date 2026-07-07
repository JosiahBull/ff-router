//! A tiny glob matcher, replacing the `globset` crate.
//!
//! `globset` compiles each glob to a full regex engine (`regex-automata` +
//! `aho-corasick`, ~250 KiB of code). We only match short URLs against a few
//! patterns, so a direct backtracking matcher is plenty. Supported syntax:
//!
//! * `*` — any run of characters, including `/`
//! * `?` — exactly one character
//! * `[abc]`, `[a-z]`, `[!abc]`/`[^abc]` — character classes with ranges and
//!   negation
//! * `{a,b,c}` — brace alternation (may nest)

/// Returns whether `text` matches the glob `pattern`.
pub fn matches(pattern: &str, text: &str) -> bool {
    let text: Vec<char> = text.chars().collect();
    expand(pattern)
        .iter()
        .any(|p| matches_flat(&p.chars().collect::<Vec<_>>(), &text))
}

/// Match a brace-free glob against `text` using classic `*`-backtracking
/// (linear in practice, no catastrophic blow-up).
fn matches_flat(pat: &[char], text: &[char]) -> bool {
    let (mut pi, mut ti) = (0usize, 0usize);
    // Position to resume from on mismatch: (pattern index after `*`, text index).
    let mut star: Option<(usize, usize)> = None;

    while ti < text.len() {
        let advanced = match pat.get(pi) {
            Some('*') => {
                star = Some((pi + 1, ti));
                pi += 1;
                true
            }
            Some(_) => {
                let (matched, len) = unit_matches(&pat[pi..], text[ti]);
                if matched {
                    pi += len;
                    ti += 1;
                }
                matched
            }
            None => false,
        };
        if !advanced {
            match star {
                Some((resume_pi, star_ti)) => {
                    pi = resume_pi;
                    ti = star_ti + 1;
                    star = Some((resume_pi, star_ti + 1));
                }
                None => return false,
            }
        }
    }

    while pat.get(pi) == Some(&'*') {
        pi += 1;
    }
    pi == pat.len()
}

/// Match the single pattern unit at the start of `pat` against `c`, returning
/// whether it matched and how many pattern chars the unit spans.
fn unit_matches(pat: &[char], c: char) -> (bool, usize) {
    match pat[0] {
        '?' => (true, 1),
        '[' => match parse_class(pat) {
            Some((negated, ranges, len)) => (in_ranges(&ranges, c) != negated, len),
            None => (c == '[', 1),
        },
        literal => (c == literal, 1),
    }
}

/// A parsed `[...]` class: negation flag, inclusive ranges, and total length
/// including the brackets.
type Class = (bool, Vec<(char, char)>, usize);

/// Parse a `[...]` class starting at `pat[0] == '['`, or `None` if there is no
/// closing `]`.
fn parse_class(pat: &[char]) -> Option<Class> {
    let mut i = 1;
    let negated = matches!(pat.get(i), Some('!' | '^'));
    if negated {
        i += 1;
    }
    let first = i;
    let mut ranges = Vec::new();
    while let Some(&c) = pat.get(i) {
        // A `]` is literal only if it is the first class member.
        if c == ']' && i > first {
            return Some((negated, ranges, i + 1));
        }
        if pat.get(i + 1) == Some(&'-') && !matches!(pat.get(i + 2), None | Some(']')) {
            ranges.push((c, pat[i + 2]));
            i += 3;
        } else {
            ranges.push((c, c));
            i += 1;
        }
    }
    None
}

fn in_ranges(ranges: &[(char, char)], c: char) -> bool {
    ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi)
}

/// Expand brace alternations into a list of brace-free patterns.
fn expand(pattern: &str) -> Vec<String> {
    match split_group(pattern) {
        None => vec![pattern.to_owned()],
        Some((prefix, options, suffix)) => {
            let tails = expand(&suffix);
            let mut out = Vec::new();
            for option in &options {
                for mid in expand(option) {
                    for tail in &tails {
                        out.push(format!("{prefix}{mid}{tail}"));
                    }
                }
            }
            out
        }
    }
}

/// Find the first top-level `{...}` group that contains a comma, returning
/// `(prefix, options, suffix)`. Brace groups without a comma are left literal.
fn split_group(s: &str) -> Option<(String, Vec<String>, String)> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '{' {
            i += 1;
            continue;
        }
        let mut depth = 0;
        let mut commas = Vec::new();
        let mut close = None;
        let mut j = i;
        while j < chars.len() {
            match chars[j] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(j);
                        break;
                    }
                }
                ',' if depth == 1 => commas.push(j),
                _ => {}
            }
            j += 1;
        }
        match (close, commas.is_empty()) {
            (Some(close), false) => {
                let prefix = chars[..i].iter().collect();
                let suffix = chars[close + 1..].iter().collect();
                let mut options = Vec::new();
                let mut start = i + 1;
                for &comma in commas.iter().chain(std::iter::once(&close)) {
                    options.push(chars[start..comma].iter().collect());
                    start = comma + 1;
                }
                return Some((prefix, options, suffix));
            }
            // No matching `}`, or a group with no comma: skip past this `{`.
            (close, _) => i = close.map_or(chars.len(), |c| c + 1),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn star_and_question() {
        assert!(matches("*", "anything/at/all"));
        assert!(matches(
            "*://*.atlassian.net/*",
            "https://team.atlassian.net/browse/X"
        ));
        assert!(matches(
            "https://github.com/partly*",
            "https://github.com/partly/repo"
        ));
        assert!(matches("a?c", "abc"));
        assert!(!matches("a?c", "ac"));
        assert!(!matches(
            "https://github.com/partly*",
            "https://github.com/other"
        ));
    }

    #[test]
    fn anchoring() {
        assert!(matches("abc", "abc"));
        assert!(!matches("abc", "abcd"));
        assert!(!matches("abc", "0abc"));
        assert!(matches("a*c", "abbbbc"));
    }

    #[test]
    fn classes() {
        assert!(matches("[abc]", "b"));
        assert!(!matches("[abc]", "d"));
        assert!(matches("[a-z]*", "hello"));
        assert!(matches("[!0-9]*", "hello"));
        assert!(!matches("[!0-9]*", "1abc"));
        assert!(matches("v[0-9].[0-9]", "v1.2"));
    }

    #[test]
    fn braces() {
        assert!(matches(
            "*.{slack,zoom,notion}.com/*",
            "https://x.slack.com/y"
        ));
        assert!(matches(
            "*.{slack,zoom,notion}.com/*",
            "https://x.notion.com/y"
        ));
        assert!(!matches("*.{slack,zoom}.com/*", "https://x.teams.com/y"));
        assert!(matches("a{b,c}d", "abd"));
        assert!(matches("a{b,{c,d}}e", "ade"));
    }
}
