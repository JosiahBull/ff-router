//! Generate the `~/.ff-router.toml` body from the wizard's selections.

use crate::discover::Profile;

/// Build the config text. `globs[i]` is the raw whitespace-separated glob line
/// entered for `profiles[i]`; `main_idx`'s profile is the everyday one and
/// gets no rule of its own. No fallback `default` key is written — links that
/// match no rule fall through to whatever profile Firefox opens on its own.
pub fn gen_config(profiles: &[Profile], main_idx: usize, globs: &[String]) -> String {
    let mut out = String::new();
    out.push_str(concat!(
        "# Links that match no rule below open in Firefox's current/default profile.\n",
        "# (Add  default = \"<label>\"  above [profiles] to force a specific fallback.)\n\n",
        "[profiles]\n",
    ));
    for p in profiles {
        out.push_str(&format!("{} = \"{}\"\n", p.label, esc(&p.dir)));
    }
    for (i, p) in profiles.iter().enumerate() {
        if i == main_idx {
            continue;
        }
        let patterns: Vec<&str> = globs[i].split_whitespace().collect();
        if patterns.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "\n[[rule]]\nprofile = \"{}\"\nglobs = [\n",
            p.label
        ));
        for pattern in patterns {
            out.push_str(&format!("    \"{}\",\n", esc(pattern)));
        }
        out.push_str("]\n");
    }
    out
}

/// Warn about globs that are not wrapped in `*` on both sides. Since matching
/// is anchored, such a glob only matches URLs that start/end exactly there,
/// which is usually not intended (e.g. `github.com/x` never matches
/// `https://github.com/x`). Returns one message per suspect glob.
pub fn glob_warnings(globs: &[String]) -> Vec<String> {
    let mut warnings = Vec::new();
    for line in globs {
        for g in line.split_whitespace() {
            let missing = match (g.starts_with('*'), g.ends_with('*')) {
                (true, true) => continue,
                (false, false) => "a leading and trailing",
                (false, true) => "a leading",
                (true, false) => "a trailing",
            };
            warnings.push(format!(
                "glob \"{g}\" is missing {missing} '*'; it is anchored and may not match as expected"
            ));
        }
    }
    warnings
}

/// Turn a profile name into a TOML-safe bare-key label: lowercase, with runs of
/// non-alphanumerics collapsed to single `-`, trimmed.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Escape a value for a TOML basic string.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::Profile;

    fn profile(name: &str, dir: &str, label: &str) -> Profile {
        Profile {
            name: name.into(),
            dir: dir.into(),
            label: label.into(),
        }
    }

    #[test]
    fn warns_on_unwrapped_globs() {
        let globs = vec!["*ok*  needslead*  *needstrail  bare".to_string()];
        let w = glob_warnings(&globs);
        assert_eq!(w.len(), 3);
        assert!(w[0].contains("needslead*") && w[0].contains("a leading '*'"));
        assert!(w[1].contains("*needstrail") && w[1].contains("a trailing '*'"));
        assert!(w[2].contains("bare") && w[2].contains("a leading and trailing '*'"));
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Home"), "home");
        assert_eq!(slug("Profile 1"), "profile-1");
        assert_eq!(slug("  Weird!! Name  "), "weird-name");
        assert_eq!(slug("!!!"), "");
    }

    #[test]
    fn generates_expected_config() {
        let profiles = vec![
            profile("Home", "dhutbqgo.default-release", "home"),
            profile("Work", "qtIifLeX.Profile 1", "work"),
        ];
        let globs = vec![String::new(), "*://*.atlassian.net/*  *partly.com/*".into()];
        let out = gen_config(&profiles, 0, &globs);

        // No forced `default = ...` fallback — unmatched links defer to Firefox.
        assert!(!out.contains("default = \"home\""));
        assert!(out.starts_with("# Links that match no rule"));
        assert!(out.contains("home = \"dhutbqgo.default-release\"\n"));
        assert!(out.contains("work = \"qtIifLeX.Profile 1\"\n"));
        assert!(out.contains("[[rule]]\nprofile = \"work\"\nglobs = [\n"));
        assert!(out.contains("    \"*://*.atlassian.net/*\",\n"));
        assert!(out.contains("    \"*partly.com/*\",\n"));
        // The everyday profile gets no rule block of its own.
        assert_eq!(out.matches("[[rule]]").count(), 1);
    }
}
