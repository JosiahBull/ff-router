//! A tiny glob matcher supporting a single wildcard: `*`, which matches any run
//! of characters (including `/` and the empty string). Every other character is
//! matched literally, and the match is anchored — the whole URL must match.
//!
//! Backed by `wildmatch`. Its single-character wildcard is bound to NUL (which
//! never appears in a URL or a config pattern), leaving `*` as the only
//! metacharacter so `?` and friends stay literal.

use wildmatch::WildMatchPattern;

/// A `wildmatch` pattern where `*` is the only wildcard; the single-character
/// wildcard slot is disabled by binding it to a char real input can't contain.
type Glob = WildMatchPattern<'*', '\0'>;

/// Returns whether `text` matches the glob `pattern`.
pub fn matches(pattern: &str, text: &str) -> bool {
    Glob::new(pattern).matches(text)
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn star_matches_any_run() {
        assert!(matches("*", "anything/at/all"));
        assert!(matches(
            "*://*.atlassian.net/*",
            "https://team.atlassian.net/browse/X"
        ));
        assert!(matches(
            "https://github.com/partly*",
            "https://github.com/partly/repo"
        ));
        assert!(!matches(
            "https://github.com/partly*",
            "https://github.com/other"
        ));
    }

    #[test]
    fn literals_are_anchored() {
        assert!(matches("abc", "abc"));
        assert!(!matches("abc", "abcd"));
        assert!(!matches("abc", "0abc"));
        assert!(matches("a*c", "abbbbc"));
        assert!(matches("a*c", "ac"));
        assert!(!matches("a*c", "ab"));
    }

    #[test]
    fn multiple_stars_and_empty() {
        assert!(matches("*b*", "aaabaaa"));
        assert!(!matches("*b*", "aaaaaa"));
        assert!(matches("**", "anything"));
        assert!(matches("", ""));
        assert!(!matches("", "x"));
    }

    #[test]
    fn question_mark_is_literal() {
        // `*` is the only wildcard; `?` matches only itself (URLs use it for
        // query strings), so it must not behave as a single-char wildcard.
        assert!(matches("a?c", "a?c"));
        assert!(!matches("a?c", "abc"));
        assert!(matches("*/search?q=*", "https://x.com/search?q=rust"));
        assert!(!matches("*/search?q=*", "https://x.com/searchXq=rust"));
    }
}
