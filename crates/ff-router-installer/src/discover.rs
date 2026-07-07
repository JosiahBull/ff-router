//! Discover Firefox profiles on this machine.
//!
//! Friendly names ("Work", "Home") come from the newer Profile Groups SQLite
//! store (queried via the system `sqlite3`); older installs are read from
//! `profiles.ini`; failing both, we fall back to the profile directory name.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::slug;

#[derive(Debug, Clone)]
pub struct Profile {
    /// Human-friendly name shown in the UI (e.g. "Work").
    pub name: String,
    /// Profile directory under `.../Firefox/Profiles/` (e.g. "qtIifLeX.Profile 1").
    pub dir: String,
    /// TOML-safe label used in the generated config (e.g. "work").
    pub label: String,
}

pub fn discover() -> Vec<Profile> {
    let ff = ff_root();
    let store = store_names(&ff);
    let ini =
        parse_profiles_ini(&std::fs::read_to_string(ff.join("profiles.ini")).unwrap_or_default());

    let mut dirs = list_profile_dirs(&ff.join("Profiles"));
    dirs.sort();

    let mut used = HashSet::new();
    let mut profiles = Vec::new();
    for dir in dirs {
        let name = store
            .get(&dir)
            .or_else(|| ini.get(&dir))
            .cloned()
            .unwrap_or_else(|| default_name(&dir).to_string());

        let mut label = slug(&name);
        if label.is_empty() {
            label = format!("profile{}", profiles.len() + 1);
        }
        let base = label.clone();
        let mut n = 2;
        while !used.insert(label.clone()) {
            label = format!("{base}-{n}");
            n += 1;
        }

        profiles.push(Profile { name, dir, label });
    }
    profiles
}

fn ff_root() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/Firefox")
}

fn list_profile_dirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

/// The directory name without its random prefix, e.g. `abc123.default` -> `default`.
fn default_name(dir: &str) -> &str {
    dir.split_once('.').map_or(dir, |(_, rest)| rest)
}

/// Map profile-directory -> friendly name from every Profile Groups store.
fn store_names(ff: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(ff.join("Profile Groups")) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sqlite") {
            continue;
        }
        let Ok(output) = Command::new("sqlite3")
            .arg(&path)
            .arg("SELECT name, path FROM Profiles;")
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some((name, path)) = line.split_once('|') {
                if let Some(dir) = path.rsplit('/').next() {
                    map.insert(dir.to_string(), name.to_string());
                }
            }
        }
    }
    map
}

/// Map profile-directory -> `Name` from a `profiles.ini` body.
pub fn parse_profiles_ini(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let (mut name, mut path): (Option<String>, Option<String>) = (None, None);
    // A trailing sentinel header flushes the final section.
    for line in text.lines().map(str::trim).chain(std::iter::once("[")) {
        if line.starts_with('[') {
            if let (Some(n), Some(p)) = (name.take(), path.take()) {
                if let Some(dir) = p.rsplit('/').next() {
                    map.insert(dir.to_string(), n);
                }
            }
        } else if let Some(v) = line.strip_prefix("Name=") {
            name = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("Path=") {
            path = Some(v.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profiles_ini() {
        let ini = "\
[Profile0]
Name=default-release
Path=Profiles/qtIifLeX.Profile 1

[Profile1]
Name=default
Path=Profiles/z9z6mukp.default
";
        let map = parse_profiles_ini(ini);
        assert_eq!(
            map.get("qtIifLeX.Profile 1").map(String::as_str),
            Some("default-release")
        );
        assert_eq!(
            map.get("z9z6mukp.default").map(String::as_str),
            Some("default")
        );
    }

    #[test]
    fn strips_directory_prefix() {
        assert_eq!(default_name("dhutbqgo.default-release"), "default-release");
        assert_eq!(default_name("no-dot"), "no-dot");
    }
}
