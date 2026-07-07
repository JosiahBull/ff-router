//! Generate the `~/.ff-router.toml` body from the wizard's selections.

use crate::discover::Profile;

/// Build the config text. `globs[i]` is the raw whitespace-separated glob line
/// entered for `profiles[i]` (unused for the default profile).
pub fn gen_config(profiles: &[Profile], default_idx: usize, globs: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "default = \"{}\"\n\n[profiles]\n",
        profiles[default_idx].label
    ));
    for p in profiles {
        out.push_str(&format!("{} = \"{}\"\n", p.label, esc(&p.dir)));
    }
    for (i, p) in profiles.iter().enumerate() {
        if i == default_idx {
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

        assert!(out.contains("default = \"home\"\n"));
        assert!(out.contains("home = \"dhutbqgo.default-release\"\n"));
        assert!(out.contains("work = \"qtIifLeX.Profile 1\"\n"));
        assert!(out.contains("[[rule]]\nprofile = \"work\"\nglobs = [\n"));
        assert!(out.contains("    \"*://*.atlassian.net/*\",\n"));
        assert!(out.contains("    \"*partly.com/*\",\n"));
        // The default profile gets no rule block of its own.
        assert_eq!(out.matches("[[rule]]").count(), 1);
    }
}
