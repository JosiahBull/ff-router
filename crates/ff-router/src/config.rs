//! Loading and evaluation of `~/.ff-router.toml`.

use std::collections::HashMap;
use std::path::PathBuf;

use globset::Glob;
use serde::Deserialize;

/// Directory holding Firefox profiles, relative to `$HOME`.
const PROFILES_DIR: &str = "Library/Application Support/Firefox/Profiles";

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Profile label used when no rule matches.
    default: String,
    /// Label -> Firefox profile directory name (or an absolute path).
    profiles: HashMap<String, String>,
    /// Ordered override rules; the first rule with a matching glob wins.
    #[serde(default)]
    rule: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
struct Rule {
    profile: String,
    globs: Vec<String>,
}

impl Config {
    /// Read and parse the config from `$HOME/.ff-router.toml`.
    pub fn load() -> Option<Self> {
        let path = home()?.join(".ff-router.toml");
        let text = std::fs::read_to_string(path).ok()?;
        toml::from_str(&text).ok()
    }

    /// Resolve `url` to a Firefox profile directory, or `None` to fall back to
    /// Firefox's own default profile.
    pub fn profile_path(&self, url: &str) -> Option<PathBuf> {
        let label = self.label_for(url);
        self.profiles.get(label).map(|value| resolve(value))
    }

    /// The profile label the url routes to (first matching rule, else default).
    fn label_for(&self, url: &str) -> &str {
        self.rule
            .iter()
            .find(|r| r.globs.iter().any(|g| matches(g, url)))
            .map_or(self.default.as_str(), |r| r.profile.as_str())
    }
}

/// Compile `pattern` and test it against `url`. Unparseable globs never match.
fn matches(pattern: &str, url: &str) -> bool {
    Glob::new(pattern).is_ok_and(|g| g.compile_matcher().is_match(url))
}

/// A bare directory name is resolved under the Firefox profiles directory; an
/// absolute path is used as-is.
fn resolve(value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        home().unwrap_or_default().join(PROFILES_DIR).join(value)
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        default = "personal"
        [profiles]
        work     = "qtIifLeX.Profile 1"
        personal = "dhutbqgo.default-release"
        [[rule]]
        profile = "work"
        globs = ["*://*.atlassian.net/*", "*.{slack,notion}.com/*"]
        [[rule]]
        profile = "work"
        globs = ["*://github.com/partly*"]
    "#;

    fn cfg() -> Config {
        toml::from_str(SAMPLE).unwrap()
    }

    #[test]
    fn first_matching_rule_wins() {
        assert_eq!(
            cfg().label_for("https://team.atlassian.net/browse/X"),
            "work"
        );
        assert_eq!(cfg().label_for("https://foo.slack.com/messages"), "work");
        assert_eq!(cfg().label_for("https://github.com/partly/repo"), "work");
    }

    #[test]
    fn unmatched_falls_back_to_default() {
        assert_eq!(cfg().label_for("https://www.youtube.com/watch"), "personal");
        assert_eq!(
            cfg().label_for("https://github.com/someone-else"),
            "personal"
        );
    }

    #[test]
    fn resolves_bare_name_and_absolute_path() {
        // SAFETY: test-only mutation of process env for a deterministic HOME.
        unsafe { std::env::set_var("HOME", "/Users/test") };
        let c = cfg();
        assert_eq!(
            c.profile_path("https://youtube.com").unwrap(),
            PathBuf::from(
                "/Users/test/Library/Application Support/Firefox/Profiles/dhutbqgo.default-release"
            ),
        );

        let abs: Config =
            toml::from_str("default = \"p\"\n[profiles]\np = \"/tmp/custom.profile\"\n").unwrap();
        assert_eq!(
            abs.profile_path("https://x").unwrap(),
            PathBuf::from("/tmp/custom.profile")
        );
    }
}
