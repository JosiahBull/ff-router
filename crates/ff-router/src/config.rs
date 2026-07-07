//! Loading and evaluation of `~/.ff-router.toml`.

use std::collections::HashMap;
use std::path::PathBuf;

use globset::GlobBuilder;
use serde::Deserialize;

/// Directory holding Firefox profiles, relative to `$HOME`.
const PROFILES_DIR: &str = "Library/Application Support/Firefox/Profiles";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Profile label used when no rule matches (falls back to Firefox's own
    /// default profile if unset).
    default: Option<String>,
    /// Label -> Firefox profile directory name (or an absolute path).
    profiles: HashMap<String, String>,
    /// Ordered override rules; the first rule with a matching glob wins.
    /// Deserialised from the `[[rule]]` array-of-tables.
    #[serde(rename = "rule")]
    rules: Vec<Rule>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Rule {
    profile: String,
    globs: Vec<String>,
}

impl Config {
    /// Read and parse the config from `$HOME/.ff-router.toml`.
    pub fn load() -> Option<Self> {
        let path = home()?.join(".ff-router.toml");
        let text = std::fs::read_to_string(path).ok()?;
        parse(&text).ok()
    }

    /// Resolve `url` to a Firefox profile directory, or `None` to fall back to
    /// Firefox's own default profile.
    pub fn profile_path(&self, url: &str) -> Option<PathBuf> {
        let label = self.label_for(url)?;
        self.profiles.get(label).map(|value| resolve(value))
    }

    /// The profile label the url routes to (first matching rule, else default).
    fn label_for(&self, url: &str) -> Option<&str> {
        self.rules
            .iter()
            .find(|r| {
                r.globs.iter().any(|g| {
                    GlobBuilder::new(g)
                        .literal_separator(false) // `*`/`?` cross `/` — URLs, not paths
                        .backslash_escape(true) // treat `\?`, `\*`, `\[` … as literals
                        .build()
                        .map(|glob| glob.compile_matcher().is_match(url))
                        .unwrap_or(false)
                })
            })
            .map(|r| r.profile.as_str())
            .or(self.default.as_deref())
    }
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
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home));
    }

    let user = std::env::var_os("USER").or_else(|| std::env::var_os("USERNAME"))?;

    let base = match std::env::consts::OS {
        "macos" => "/Users",
        "windows" => "C:\\Users",
        _ => "/home",
    };

    Some(PathBuf::from(base).join(user))
}

/// Parse the TOML `input` into a [`Config`]. Unknown tables and keys are
/// ignored (serde skips fields the structs don't declare).
fn parse(input: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        default = "personal"   # fall-back profile

        [profiles]
        work     = "qtIifLeX.Profile 1"
        personal = "dhutbqgo.default-release"

        [[rule]]
        profile = "work"
        globs = [
            "*://*.atlassian.net/*",
            "*.slack.com/*",
        ]

        [[rule]]
        profile = "work"
        globs = ["*://github.com/partly*"]
    "#;

    fn cfg() -> Config {
        parse(SAMPLE).unwrap()
    }

    #[test]
    fn first_matching_rule_wins() {
        assert_eq!(
            cfg().label_for("https://team.atlassian.net/browse/X"),
            Some("work")
        );
        assert_eq!(
            cfg().label_for("https://foo.slack.com/messages"),
            Some("work")
        );
        assert_eq!(
            cfg().label_for("https://github.com/partly/repo"),
            Some("work")
        );
    }

    #[test]
    fn unmatched_falls_back_to_default() {
        assert_eq!(
            cfg().label_for("https://www.youtube.com/watch"),
            Some("personal")
        );
        assert_eq!(
            cfg().label_for("https://github.com/someone-else"),
            Some("personal")
        );
    }

    #[test]
    fn no_default_and_no_match_is_none() {
        let c = parse("[[rule]]\nprofile = \"work\"\nglobs = [\"*.work.com/*\"]\n").unwrap();
        assert_eq!(c.label_for("https://example.com"), None);
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

        let abs = parse("default = \"p\"\n[profiles]\np = \"/tmp/custom.profile\"\n").unwrap();
        assert_eq!(
            abs.profile_path("https://x").unwrap(),
            PathBuf::from("/tmp/custom.profile")
        );
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse("default =").is_err());
        assert!(parse("[profiles").is_err());
        assert!(parse("default = \"unterminated").is_err());
        assert!(parse("globs = [\"a\" \"b\"]").is_err());
    }

    #[test]
    fn ignores_unknown_tables_and_keys() {
        let c = parse("nope = \"x\"\n[other]\nk = \"v\"\ndefault = \"p\"\n").unwrap();
        assert_eq!(c.label_for("https://anything"), None);
        // `default` sits at root before `[other]`, so it still applies.
        let c = parse("default = \"p\"\n[other]\nk = \"v\"\n").unwrap();
        assert_eq!(c.label_for("https://anything"), Some("p"));
    }
}
