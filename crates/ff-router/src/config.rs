//! Loading and evaluation of `~/.ff-router.toml`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use globset::GlobBuilder;
use serde::Deserialize;

/// Directory holding Firefox profiles, relative to `$HOME`.
const PROFILES_DIR: &str = "Library/Application Support/Firefox/Profiles";

/// The application that asked macOS to open a URL, as reported by the Apple
/// Event that delivered it. Either field may be absent (e.g. a process without
/// an `Info.plist`), and callers pass `None` when there is no discernible
/// sender at all (terminal `open`, Spotlight, direct invocation).
#[derive(Debug, Default)]
pub struct Opener {
    /// The opener's `CFBundleIdentifier`, e.g. `com.tinyspeck.slackmacgap`.
    pub bundle_id: Option<String>,
    /// The opener's localized display name, e.g. `Slack`.
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// When true, append each routing decision (URL, opener, matched rule) to
    /// `~/.ff-router.log`. Off by default.
    debug: bool,
    /// Profile label used when no rule matches (falls back to Firefox's own
    /// default profile if unset).
    default: Option<String>,
    /// Label -> Firefox profile directory name (or an absolute path).
    profiles: HashMap<String, String>,
    /// Ordered override rules; the first matching rule wins. Deserialised from
    /// the `[[rule]]` array-of-tables.
    #[serde(rename = "rule")]
    rules: Vec<Rule>,
}

/// The outcome of routing one URL: the profile to launch with, plus a
/// human-readable explanation of how the decision was reached (for the debug
/// log). The caller logs the OS-provided context (URL, opener) alongside it.
pub struct Decision {
    /// Firefox profile directory to launch with, or `None` for Firefox's own
    /// default profile.
    pub profile: Option<PathBuf>,
    /// One-line description of which rule matched and where it routed.
    pub explanation: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Rule {
    profile: String,
    /// Globs matched against the URL. Empty means "any URL".
    globs: Vec<String>,
    /// Globs matched against the opening application's bundle id *and* its
    /// localized name; the rule applies when at least one glob matches either.
    /// Empty means "any source". A source that is set only matches when the
    /// opener is known, so URLs opened without a sender skip such rules.
    source: Vec<String>,
}

impl Config {
    /// Read and parse the config from `$HOME/.ff-router.toml`.
    pub fn load() -> Option<Self> {
        let path = home()?.join(".ff-router.toml");
        let text = std::fs::read_to_string(path).ok()?;
        parse(&text).ok()
    }

    /// Whether routing decisions should be appended to the debug log.
    pub fn is_debug(&self) -> bool {
        self.debug
    }

    /// Resolve `url` (opened by `opener`, if known) to a Firefox profile
    /// directory, or `None` to fall back to Firefox's own default profile.
    pub fn profile_path(&self, url: &str, opener: Option<&Opener>) -> Option<PathBuf> {
        self.profile_for_label(self.resolve_label(url, opener).1)
    }

    /// Like [`Config::profile_path`], but also explains how the decision was
    /// reached. Used on the debug path only (it always allocates the message).
    pub fn decide(&self, url: &str, opener: Option<&Opener>) -> Decision {
        let (rule, label) = self.resolve_label(url, opener);
        let profile = self.profile_for_label(label);
        let explanation = explain(rule, label, profile.as_deref());
        Decision {
            profile,
            explanation,
        }
    }

    /// The first matching rule's index and profile label; falls back to the
    /// `default` label (with no rule index) when nothing matches. `(None, None)`
    /// when there is no match and no default.
    fn resolve_label(&self, url: &str, opener: Option<&Opener>) -> (Option<usize>, Option<&str>) {
        match self.rules.iter().position(|r| r.matches(url, opener)) {
            Some(i) => (Some(i), Some(self.rules[i].profile.as_str())),
            None => (None, self.default.as_deref()),
        }
    }

    /// The profile label the url routes to (first matching rule, else default).
    #[cfg(test)]
    fn label_for(&self, url: &str, opener: Option<&Opener>) -> Option<&str> {
        self.resolve_label(url, opener).1
    }

    /// Resolve a profile label to its directory, `None` if the label is unset
    /// or absent from `[profiles]`.
    fn profile_for_label(&self, label: Option<&str>) -> Option<PathBuf> {
        label
            .and_then(|l| self.profiles.get(l))
            .map(|value| resolve(value))
    }
}

/// Build the debug-log explanation of a routing decision.
fn explain(rule: Option<usize>, label: Option<&str>, profile: Option<&Path>) -> String {
    let target = match (label, profile) {
        (Some(l), Some(p)) => format!("profile \"{l}\" ({})", p.display()),
        (Some(l), None) => {
            format!("profile \"{l}\" (not found in [profiles]; using Firefox default)")
        }
        (None, _) => "Firefox default profile".to_string(),
    };
    match rule {
        Some(i) => format!("matched rule #{i} -> {target}"),
        None if label.is_some() => format!("no rule matched -> default {target}"),
        None => format!("no rule matched, no default -> {target}"),
    }
}

impl Rule {
    /// Whether this rule applies to `url` opened by `opener`. Each dimension
    /// that is set (`globs`, `source`) must match; an empty list is "no
    /// constraint". A rule with neither set matches nothing.
    fn matches(&self, url: &str, opener: Option<&Opener>) -> bool {
        if self.globs.is_empty() && self.source.is_empty() {
            return false;
        }
        let url_ok = self.globs.is_empty() || self.globs.iter().any(|g| glob_match(g, url));
        let source_ok = self.source.is_empty() || source_matches(&self.source, opener);
        url_ok && source_ok
    }
}

/// Whether any `source` glob matches the opener's bundle id or localized name.
/// A `source` constraint never matches when the opener is unknown.
fn source_matches(source: &[String], opener: Option<&Opener>) -> bool {
    let Some(op) = opener else { return false };
    source.iter().any(|g| {
        op.bundle_id.as_deref().is_some_and(|b| glob_match(g, b))
            || op.name.as_deref().is_some_and(|n| glob_match(g, n))
    })
}

/// Match `value` against one glob using the router's dialect: `*`/`?` cross `/`
/// (URLs and bundle ids are not paths), `\` escapes metacharacters, and
/// matching is case-insensitive. A malformed pattern never matches.
fn glob_match(pattern: &str, value: &str) -> bool {
    GlobBuilder::new(pattern)
        .literal_separator(false)
        .backslash_escape(true)
        .case_insensitive(true)
        .build()
        .map(|glob| glob.compile_matcher().is_match(value))
        .unwrap_or(false)
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

pub(crate) fn home() -> Option<PathBuf> {
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
            cfg().label_for("https://team.atlassian.net/browse/X", None),
            Some("work")
        );
        assert_eq!(
            cfg().label_for("https://foo.slack.com/messages", None),
            Some("work")
        );
        assert_eq!(
            cfg().label_for("https://github.com/partly/repo", None),
            Some("work")
        );
    }

    #[test]
    fn unmatched_falls_back_to_default() {
        assert_eq!(
            cfg().label_for("https://www.youtube.com/watch", None),
            Some("personal")
        );
        assert_eq!(
            cfg().label_for("https://github.com/someone-else", None),
            Some("personal")
        );
    }

    #[test]
    fn no_default_and_no_match_is_none() {
        let c = parse("[[rule]]\nprofile = \"work\"\nglobs = [\"*.work.com/*\"]\n").unwrap();
        assert_eq!(c.label_for("https://example.com", None), None);
    }

    fn opener(bundle_id: &str, name: &str) -> Opener {
        Opener {
            bundle_id: Some(bundle_id.into()),
            name: Some(name.into()),
        }
    }

    #[test]
    fn source_glob_matches_bundle_id_or_name() {
        let c = parse(
            "default = \"personal\"\n\
             [[rule]]\n\
             profile = \"work\"\n\
             source = [\"com.tinyspeck.*\", \"Microsoft Outlook\"]\n",
        )
        .unwrap();

        // Matches on bundle id.
        let slack = opener("com.tinyspeck.slackmacgap", "Slack");
        assert_eq!(
            c.label_for("https://example.com", Some(&slack)),
            Some("work")
        );

        // Matches on localized name even though the bundle id doesn't.
        let outlook = opener("com.microsoft.Outlook", "Microsoft Outlook");
        assert_eq!(
            c.label_for("https://example.com", Some(&outlook)),
            Some("work")
        );

        // A different app falls through to the default.
        let mail = opener("com.apple.mail", "Mail");
        assert_eq!(
            c.label_for("https://example.com", Some(&mail)),
            Some("personal")
        );

        // Unknown sender (terminal/Spotlight) can't satisfy a source rule.
        assert_eq!(c.label_for("https://example.com", None), Some("personal"));
    }

    #[test]
    fn debug_flag_defaults_off_and_parses() {
        assert!(!parse("default = \"p\"").unwrap().is_debug());
        assert!(parse("debug = true\ndefault = \"p\"").unwrap().is_debug());
    }

    #[test]
    fn decide_explains_a_rule_match() {
        let d = cfg().decide("https://foo.slack.com/x", None);
        assert!(d.profile.is_some());
        assert!(
            d.explanation
                .starts_with("matched rule #0 -> profile \"work\""),
            "{}",
            d.explanation
        );
    }

    #[test]
    fn decide_explains_default_and_missing_profile() {
        let d = cfg().decide("https://unmatched.example/x", None);
        assert!(
            d.explanation
                .starts_with("no rule matched -> default profile \"personal\""),
            "{}",
            d.explanation
        );

        // A match whose label is absent from [profiles] → Firefox default.
        let c = parse("[[rule]]\nprofile = \"ghost\"\nglobs = [\"*\"]\n").unwrap();
        let d = c.decide("https://anything", None);
        assert!(d.profile.is_none());
        assert!(
            d.explanation.contains("not found in [profiles]"),
            "{}",
            d.explanation
        );
    }

    #[test]
    fn decide_explains_no_match_no_default() {
        let c = parse("[[rule]]\nprofile = \"work\"\nglobs = [\"*.work.com/*\"]\n").unwrap();
        let d = c.decide("https://example.com", None);
        assert!(d.profile.is_none());
        assert_eq!(
            d.explanation,
            "no rule matched, no default -> Firefox default profile"
        );
    }

    #[test]
    fn source_and_globs_are_anded() {
        let c = parse(
            "default = \"personal\"\n\
             [[rule]]\n\
             profile = \"work\"\n\
             globs = [\"*://*.github.com/*\"]\n\
             source = [\"com.tinyspeck.*\"]\n",
        )
        .unwrap();
        let slack = opener("com.tinyspeck.slackmacgap", "Slack");
        let mail = opener("com.apple.mail", "Mail");

        // URL *and* source match.
        assert_eq!(
            c.label_for("https://api.github.com/x", Some(&slack)),
            Some("work")
        );
        // Source matches but URL doesn't.
        assert_eq!(
            c.label_for("https://example.com", Some(&slack)),
            Some("personal")
        );
        // URL matches but source doesn't.
        assert_eq!(
            c.label_for("https://api.github.com/x", Some(&mail)),
            Some("personal")
        );
    }

    #[test]
    fn resolves_bare_name_and_absolute_path() {
        // SAFETY: test-only mutation of process env for a deterministic HOME.
        unsafe { std::env::set_var("HOME", "/Users/test") };
        let c = cfg();
        assert_eq!(
            c.profile_path("https://youtube.com", None).unwrap(),
            PathBuf::from(
                "/Users/test/Library/Application Support/Firefox/Profiles/dhutbqgo.default-release"
            ),
        );

        let abs = parse("default = \"p\"\n[profiles]\np = \"/tmp/custom.profile\"\n").unwrap();
        assert_eq!(
            abs.profile_path("https://x", None).unwrap(),
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
        assert_eq!(c.label_for("https://anything", None), None);
        // `default` sits at root before `[other]`, so it still applies.
        let c = parse("default = \"p\"\n[other]\nk = \"v\"\n").unwrap();
        assert_eq!(c.label_for("https://anything", None), Some("p"));
    }
}
