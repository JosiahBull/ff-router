//! Loading and evaluation of `~/.ff-router.toml`.
//!
//! The config uses a tiny TOML subset (top-level string keys, a `[profiles]`
//! string table, and `[[rule]]` array-of-tables), so we parse it by hand
//! rather than pulling in `toml` + `serde` (~50 KiB of code plus the
//! `serde_derive`/`syn` build-time cost).

use std::collections::HashMap;
use std::iter::Peekable;
use std::path::PathBuf;
use std::str::Chars;

use crate::glob;

/// Directory holding Firefox profiles, relative to `$HOME`.
const PROFILES_DIR: &str = "Library/Application Support/Firefox/Profiles";

#[derive(Debug, Default)]
pub struct Config {
    /// Profile label used when no rule matches (falls back to Firefox's own
    /// default profile if unset).
    default: Option<String>,
    /// Label -> Firefox profile directory name (or an absolute path).
    profiles: HashMap<String, String>,
    /// Ordered override rules; the first rule with a matching glob wins.
    rules: Vec<Rule>,
}

#[derive(Debug, Default)]
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
            .find(|r| r.globs.iter().any(|g| glob::matches(g, url)))
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

// --- Minimal TOML parser -------------------------------------------------

#[derive(Debug, PartialEq)]
enum Tok {
    LBracket,
    RBracket,
    Eq,
    Comma,
    Str(String),
    Ident(String),
}

enum Value {
    Str(String),
    Arr(Vec<String>),
}

impl Value {
    fn into_str(self) -> Result<String, String> {
        match self {
            Value::Str(s) => Ok(s),
            Value::Arr(_) => Err("expected a string".into()),
        }
    }

    fn into_arr(self) -> Result<Vec<String>, String> {
        match self {
            Value::Arr(a) => Ok(a),
            Value::Str(_) => Err("expected an array".into()),
        }
    }
}

fn parse(input: &str) -> Result<Config, String> {
    let toks = tokenize(input)?;
    let mut config = Config::default();
    let mut section = Section::Root;
    let mut p = 0;

    while let Some(tok) = toks.get(p) {
        match tok {
            Tok::LBracket => section = parse_header(&toks, &mut p, &mut config)?,
            Tok::Ident(key) => {
                let key = key.clone();
                p += 1;
                expect(&toks, &mut p, &Tok::Eq)?;
                let value = parse_value(&toks, &mut p)?;
                apply(&mut config, &section, &key, value)?;
            }
            other => return Err(format!("unexpected token {other:?}")),
        }
    }
    Ok(config)
}

enum Section {
    Root,
    Profiles,
    Rule,
    Ignore,
}

/// Parse a `[table]` or `[[array-table]]` header and return the new section.
fn parse_header(toks: &[Tok], p: &mut usize, config: &mut Config) -> Result<Section, String> {
    *p += 1; // consume '['
    let array_table = toks.get(*p) == Some(&Tok::LBracket);
    if array_table {
        *p += 1;
    }
    let name = match toks.get(*p) {
        Some(Tok::Ident(n)) => n.clone(),
        _ => return Err("expected a table name".into()),
    };
    *p += 1;
    expect(toks, p, &Tok::RBracket)?;
    if array_table {
        expect(toks, p, &Tok::RBracket)?;
    }
    Ok(match (array_table, name.as_str()) {
        (false, "profiles") => Section::Profiles,
        (true, "rule") => {
            config.rules.push(Rule::default());
            Section::Rule
        }
        _ => Section::Ignore,
    })
}

/// Store a parsed key/value into the current section. Unknown keys are ignored.
fn apply(config: &mut Config, section: &Section, key: &str, value: Value) -> Result<(), String> {
    match section {
        Section::Root if key == "default" => config.default = Some(value.into_str()?),
        Section::Profiles => {
            config.profiles.insert(key.to_owned(), value.into_str()?);
        }
        Section::Rule => {
            let rule = config.rules.last_mut().expect("rule pushed by header");
            match key {
                "profile" => rule.profile = value.into_str()?,
                "globs" => rule.globs = value.into_arr()?,
                _ => {}
            }
        }
        Section::Root | Section::Ignore => {}
    }
    Ok(())
}

fn parse_value(toks: &[Tok], p: &mut usize) -> Result<Value, String> {
    match toks.get(*p) {
        Some(Tok::Str(s)) => {
            let s = s.clone();
            *p += 1;
            Ok(Value::Str(s))
        }
        Some(Tok::LBracket) => {
            *p += 1;
            let mut items = Vec::new();
            loop {
                match toks.get(*p) {
                    Some(Tok::RBracket) => {
                        *p += 1;
                        return Ok(Value::Arr(items));
                    }
                    Some(Tok::Str(s)) => {
                        items.push(s.clone());
                        *p += 1;
                        match toks.get(*p) {
                            Some(Tok::Comma) => *p += 1,
                            Some(Tok::RBracket) => {
                                *p += 1;
                                return Ok(Value::Arr(items));
                            }
                            _ => return Err("expected ',' or ']'".into()),
                        }
                    }
                    _ => return Err("expected a string or ']'".into()),
                }
            }
        }
        _ => Err("expected a value".into()),
    }
}

fn expect(toks: &[Tok], p: &mut usize, want: &Tok) -> Result<(), String> {
    if toks.get(*p) == Some(want) {
        *p += 1;
        Ok(())
    } else {
        Err(format!("expected {want:?}"))
    }
}

fn tokenize(input: &str) -> Result<Vec<Tok>, String> {
    let mut toks = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                chars.next();
            }
            '#' => {
                while chars.peek().is_some_and(|&c| c != '\n') {
                    chars.next();
                }
            }
            '[' => push(&mut chars, &mut toks, Tok::LBracket),
            ']' => push(&mut chars, &mut toks, Tok::RBracket),
            '=' => push(&mut chars, &mut toks, Tok::Eq),
            ',' => push(&mut chars, &mut toks, Tok::Comma),
            '"' => {
                chars.next();
                toks.push(Tok::Str(basic_string(&mut chars)?));
            }
            '\'' => {
                chars.next();
                toks.push(Tok::Str(literal_string(&mut chars)?));
            }
            c if is_ident(c) => toks.push(Tok::Ident(ident(&mut chars))),
            other => return Err(format!("unexpected character {other:?}")),
        }
    }
    Ok(toks)
}

fn push(chars: &mut Peekable<Chars>, toks: &mut Vec<Tok>, tok: Tok) {
    chars.next();
    toks.push(tok);
}

fn basic_string(chars: &mut Peekable<Chars>) -> Result<String, String> {
    let mut s = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(s),
            '\\' => {
                let escaped = chars.next().ok_or("unterminated escape")?;
                s.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
            }
            c => s.push(c),
        }
    }
    Err("unterminated string".into())
}

fn literal_string(chars: &mut Peekable<Chars>) -> Result<String, String> {
    let mut s = String::new();
    for c in chars.by_ref() {
        if c == '\'' {
            return Ok(s);
        }
        s.push(c);
    }
    Err("unterminated string".into())
}

fn ident(chars: &mut Peekable<Chars>) -> String {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if is_ident(c) {
            s.push(c);
            chars.next();
        } else {
            break;
        }
    }
    s
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
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
