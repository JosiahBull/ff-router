//! The ordered list of actions the installer performs, and how to describe,
//! inspect (for conflicts), and execute each one.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

const APP: &str = "Firefox Router.app";
const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
const BUNDLE_ID: &str = "com.josiahbull.ff-router";

/// A single install step.
pub enum Action {
    /// Write text to a file (the config).
    WriteFile { path: PathBuf, contents: String },
    /// Move a directory into `dir` (the app bundle into ~/Applications).
    MoveInto { from: PathBuf, dir: PathBuf },
    /// Set the executable bit on a file.
    MakeExecutable { path: PathBuf },
    /// Run a command.
    Run {
        label: String,
        program: String,
        args: Vec<String>,
    },
    /// Write a LaunchAgent plist and (re)load it, so the resident router starts
    /// at login (and immediately) and links are routed by a warm process.
    InstallLoginItem { plist: PathBuf, contents: String },
    /// Remove the build artifacts (dist/ and target/).
    RemoveArtifacts { root: PathBuf, dist: PathBuf },
}

/// Whether an action's target already exists, and if it can be diffed.
pub enum Conflict {
    None,
    /// Target exists but has no useful textual representation to diff.
    Exists(PathBuf),
    /// Target file exists; `existing` vs `proposed` can be shown as a diff.
    Text {
        path: PathBuf,
        existing: String,
        proposed: String,
    },
}

/// An action paired with the user's decision to apply it or not.
pub struct Decided {
    pub action: Action,
    pub apply: bool,
}

/// Build the plan. The app bundle is expected to already exist in `dist/`
/// (scripts/install.sh builds it before launching the installer).
pub fn build(root: &Path, home: &Path, config: String) -> Vec<Action> {
    let apps = home.join("Applications");
    let installed = apps.join(APP);
    vec![
        Action::WriteFile {
            path: home.join(".ff-router.toml"),
            contents: config,
        },
        Action::MoveInto {
            from: root.join("dist").join(APP),
            dir: apps,
        },
        Action::MakeExecutable {
            path: installed.join("Contents/MacOS/ff-router"),
        },
        Action::Run {
            label: "Register the app with Launch Services".into(),
            program: LSREGISTER.into(),
            args: vec!["-f".into(), installed.to_string_lossy().into_owned()],
        },
        Action::InstallLoginItem {
            plist: home
                .join("Library/LaunchAgents")
                .join(format!("{BUNDLE_ID}.plist")),
            contents: login_item_plist(&installed.join("Contents/MacOS/ff-router")),
        },
        Action::Run {
            label: "Request to become your default browser (macOS will ask you to confirm)".into(),
            program: installed
                .join("Contents/MacOS/ff-router")
                .to_string_lossy()
                .into_owned(),
            args: vec!["--set-default".into()],
        },
        Action::RemoveArtifacts {
            root: root.to_path_buf(),
            dist: root.join("dist"),
        },
    ]
}

impl Action {
    /// A one-line "I am going to …" description.
    pub fn summary(&self) -> String {
        match self {
            Action::WriteFile { path, .. } => {
                format!("Write the configuration to {}", home_relative(path))
            }
            Action::MoveInto { from, dir } => format!(
                "Move {} into {}",
                from.file_name().unwrap_or_default().to_string_lossy(),
                home_relative(dir)
            ),
            Action::MakeExecutable { path } => {
                format!(
                    "Set executable permission (chmod 755) on {}",
                    home_relative(path)
                )
            }
            Action::Run { label, .. } => label.clone(),
            Action::InstallLoginItem { .. } => {
                "Start the router at login so links open instantly (install a LaunchAgent)".into()
            }
            Action::RemoveArtifacts { .. } => "Remove build artifacts (dist/ and target/)".into(),
        }
    }

    /// A second detail line (path, command line, …).
    pub fn detail(&self) -> String {
        match self {
            Action::WriteFile { path, contents } => {
                format!(
                    "{} ({} lines)",
                    home_relative(path),
                    contents.lines().count()
                )
            }
            Action::MoveInto { from, dir } => {
                format!("{}  →  {}", home_relative(from), home_relative(dir))
            }
            Action::MakeExecutable { path } => format!("chmod 755 {}", home_relative(path)),
            Action::Run { program, args, .. } => format!("$ {program} {}", args.join(" ")),
            Action::InstallLoginItem { plist, .. } => {
                format!("launchctl bootstrap gui/$UID {}", home_relative(plist))
            }
            Action::RemoveArtifacts { dist, root } => {
                format!(
                    "rm -rf {} && cargo clean in {}",
                    home_relative(dist),
                    home_relative(root)
                )
            }
        }
    }

    /// Whether the action's target already exists (and can be diffed).
    pub fn conflict(&self) -> Conflict {
        match self {
            Action::WriteFile { path, contents } if path.exists() => Conflict::Text {
                path: path.clone(),
                existing: std::fs::read_to_string(path).unwrap_or_default(),
                proposed: contents.clone(),
            },
            Action::InstallLoginItem { plist, contents } if plist.exists() => Conflict::Text {
                path: plist.clone(),
                existing: std::fs::read_to_string(plist).unwrap_or_default(),
                proposed: contents.clone(),
            },
            Action::MoveInto { from, dir } => {
                let target = dir.join(from.file_name().unwrap_or_default());
                if target.exists() {
                    Conflict::Exists(target)
                } else {
                    Conflict::None
                }
            }
            _ => Conflict::None,
        }
    }

    /// Perform the action (overwriting any existing target).
    pub fn execute(&self) -> io::Result<()> {
        match self {
            Action::WriteFile { path, contents } => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, contents)
            }
            Action::MoveInto { from, dir } => {
                std::fs::create_dir_all(dir)?;
                let target = dir.join(from.file_name().unwrap_or_default());
                let _ = std::fs::remove_dir_all(&target);
                check(Command::new("mv").arg(from).arg(&target).status()?)
            }
            Action::MakeExecutable { path } => set_executable(path),
            Action::Run { program, args, .. } => check(Command::new(program).args(args).status()?),
            Action::InstallLoginItem { plist, contents } => {
                if let Some(parent) = plist.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(plist, contents)?;
                load_login_item(plist)
            }
            Action::RemoveArtifacts { root, dist } => {
                let _ = std::fs::remove_dir_all(dist);
                let _ = Command::new("cargo")
                    .arg("clean")
                    .current_dir(root)
                    .status();
                Ok(())
            }
        }
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if !path.exists() {
        return Ok(());
    }
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn check(status: ExitStatus) -> io::Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("command failed ({status})")))
    }
}

/// (Re)load the LaunchAgent using the modern per-user `launchctl` domain API.
///
/// The legacy `load -w`/`unload` subcommands are deprecated and, on recent
/// macOS, print a confusing "Input/output error" even on a fresh install where
/// nothing is loaded yet (they are the reason launchctl itself suggests running
/// `bootout`). `bootout`/`bootstrap` in the `gui/<uid>` domain are the
/// supported replacements and stay quiet.
fn load_login_item(plist: &Path) -> io::Result<()> {
    let domain = format!("gui/{}", current_uid());
    let service = format!("{domain}/{BUNDLE_ID}");

    // Best-effort unload of any previous instance so `bootstrap` doesn't fail
    // with "service already loaded". "Not loaded" is an expected, harmless
    // outcome, so ignore the status and swallow the legacy-compat noise.
    let _ = Command::new("launchctl")
        .arg("bootout")
        .arg(&domain)
        .arg(plist)
        .stderr(Stdio::null())
        .status();

    // Clear any persisted "disabled" flag (the legacy `-w`), so an agent the
    // user previously disabled still gets (re)started.
    let _ = Command::new("launchctl")
        .arg("enable")
        .arg(&service)
        .stderr(Stdio::null())
        .status();

    check(
        Command::new("launchctl")
            .arg("bootstrap")
            .arg(&domain)
            .arg(plist)
            .status()?,
    )
}

/// The current user's numeric id, for the `gui/<uid>` launchd domain. macOS
/// doesn't export `$UID`, so ask `id -u`; an empty result makes `bootstrap`
/// fail loudly, which beats silently loading into the wrong domain.
fn current_uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// The LaunchAgent plist that starts the resident router at login (and now,
/// once loaded). `program` is the absolute path to the installed executable.
///
/// `RunAtLoad` starts it; `LimitLoadToSessionType = Aqua` keeps it to the GUI
/// login session (it needs Launch Services / the window server); `Interactive`
/// marks it as a foreground-quality process rather than a batch daemon.
fn login_item_plist(program: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{BUNDLE_ID}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{program}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
</dict>
</plist>
"#,
        program = xml_escape(&program.display().to_string())
    )
}

/// Escape the characters that would break out of an XML text node. macOS
/// usernames can't contain these, but the path is user-derived so escape it.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render a path with `$HOME` shortened to `~` for display.
pub fn home_relative(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_plan() {
        let actions = build(Path::new("/repo"), Path::new("/home/u"), "cfg".into());
        assert_eq!(actions.len(), 7);
        assert!(matches!(actions[0], Action::WriteFile { .. }));
        assert!(actions[0].summary().contains(".ff-router.toml"));
        assert!(matches!(actions[1], Action::MoveInto { .. }));
        assert!(actions[1].summary().contains("Firefox Router.app"));
        assert!(matches!(actions[2], Action::MakeExecutable { .. }));
        assert!(actions[2].summary().contains("chmod 755"));
        assert!(matches!(actions[3], Action::Run { .. }));
        assert!(actions[3].detail().starts_with("$ "));
        assert!(matches!(actions[4], Action::InstallLoginItem { .. }));
        assert!(actions[4].summary().contains("login"));
        assert!(actions[4].detail().contains("launchctl bootstrap"));
        assert!(matches!(actions[5], Action::Run { .. }));
        assert!(actions[5].summary().contains("default browser"));
        assert!(matches!(actions[6], Action::RemoveArtifacts { .. }));
    }

    #[test]
    fn login_item_plist_points_at_installed_binary() {
        let plist = login_item_plist(Path::new(
            "/home/u/Applications/Firefox Router.app/Contents/MacOS/ff-router",
        ));
        assert!(plist.contains("<string>com.josiahbull.ff-router</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains(
            "<string>/home/u/Applications/Firefox Router.app/Contents/MacOS/ff-router</string>"
        ));
    }

    #[test]
    fn login_item_plist_escapes_xml_in_path() {
        let plist = login_item_plist(Path::new("/home/a&b/MacOS/ff-router"));
        assert!(plist.contains("/home/a&amp;b/MacOS/ff-router"));
        assert!(!plist.contains("a&b"));
    }

    #[test]
    fn write_file_conflict_is_textual() {
        let dir = std::env::temp_dir().join(format!("ffr-plan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cfg.toml");
        std::fs::write(&path, "old\n").unwrap();

        let action = Action::WriteFile {
            path: path.clone(),
            contents: "new\n".into(),
        };
        match action.conflict() {
            Conflict::Text {
                existing, proposed, ..
            } => {
                assert_eq!(existing, "old\n");
                assert_eq!(proposed, "new\n");
            }
            _ => panic!("expected a textual conflict"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
