//! A tiny glob matcher supporting a single wildcard: `*`, which matches any run
//! of characters (including `/` and the empty string). Every other character is
//! matched literally, and the match is anchored — the whole URL must match.

/// Returns whether `text` matches the glob `pattern`.
pub fn matches(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();

    let (mut pi, mut ti) = (0usize, 0usize);
    // Position to resume from on mismatch: (pattern index after `*`, text index).
    let mut star: Option<(usize, usize)> = None;

    while ti < text.len() {
        if pat.get(pi) == Some(&'*') {
            star = Some((pi + 1, ti));
            pi += 1;
        } else if pat.get(pi) == Some(&text[ti]) {
            pi += 1;
            ti += 1;
        } else if let Some((resume_pi, star_ti)) = star {
            pi = resume_pi;
            ti = star_ti + 1;
            star = Some((resume_pi, star_ti + 1));
        } else {
            return false;
        }
    }

    while pat.get(pi) == Some(&'*') {
        pi += 1;
    }
    pi == pat.len()
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
}
