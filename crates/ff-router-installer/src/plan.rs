//! The ordered list of actions the installer performs, and how to describe,
//! inspect (for conflicts), and execute each one.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

const APP: &str = "Firefox Router.app";
const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
const BUNDLE_ID: &str = "com.josiahbull.ff-router";
/// GitHub `owner/repo` the release binaries are published under.
const REPO: &str = "josiahbull/ff-router";
/// The bundle's `Info.plist`, baked in at compile time so the installer can
/// assemble the app without a repo checkout on the user's machine.
const INFO_PLIST: &str = include_str!("../../../Info.plist");
/// The LaunchAgent plist that starts the resident router at login, baked in at
/// compile time. The `{program}` placeholder is replaced with the absolute path
/// to the installed executable when the login item is written.
const LOGIN_ITEM_PLIST: &str = include_str!("../../../LaunchAgent.plist");

/// Where the `ff-router` executable comes from when assembling the app bundle.
#[derive(Clone)]
pub enum AppSource {
    /// Download the matching release asset from GitHub (the normal path).
    Download { version: String },
    /// Use a locally-built binary instead of downloading (dev; `FF_ROUTER_BIN`).
    Local { binary: PathBuf },
}

/// A single install step.
pub enum Action {
    /// Write text to a file.
    WriteFile { path: PathBuf, contents: String },
    /// Obtain `ff-router` (download or copy) and assemble a signed
    /// `Firefox Router.app` inside `staging`.
    FetchApp { source: AppSource, staging: PathBuf },
    /// Move a directory into `dir`.
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
}

/// The URL of the `ff-router` release asset for a given version.
fn download_url(version: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/v{version}/ff-router")
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

/// Build the plan. The app bundle is fetched and assembled into `staging` by
/// the [`Action::FetchApp`] step before it is moved into ~/Applications.
pub fn build(source: AppSource, home: &Path, config: String, staging: &Path) -> Vec<Action> {
    let apps = home.join("Applications");
    let installed = apps.join(APP);
    vec![
        Action::WriteFile {
            path: home.join(".ff-router.toml"),
            contents: config,
        },
        Action::FetchApp {
            source,
            staging: staging.to_path_buf(),
        },
        Action::MoveInto {
            from: staging.join(APP),
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
    ]
}

impl Action {
    /// A one-line "I am going to …" description.
    pub fn summary(&self) -> String {
        match self {
            Action::WriteFile { path, .. } => {
                format!("Write the configuration to {}", home_relative(path))
            }
            Action::FetchApp { source, .. } => match source {
                AppSource::Download { version } => {
                    format!("Download Firefox Router {version} from GitHub and assemble the app")
                }
                AppSource::Local { .. } => {
                    "Assemble Firefox Router.app from the local build".into()
                }
            },
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
            Action::FetchApp { source, staging } => match source {
                AppSource::Download { version } => download_url(version),
                AppSource::Local { binary } => {
                    format!("{}  →  {}", home_relative(binary), home_relative(staging))
                }
            },
            Action::MoveInto { from, dir } => {
                format!("{}  →  {}", home_relative(from), home_relative(dir))
            }
            Action::MakeExecutable { path } => format!("chmod 755 {}", home_relative(path)),
            Action::Run { program, args, .. } => format!("$ {program} {}", args.join(" ")),
            Action::InstallLoginItem { plist, .. } => {
                format!("launchctl bootstrap gui/$UID {}", home_relative(plist))
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
            Action::FetchApp { source, staging } => fetch_app(source, staging),
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
        }
    }
}

/// Obtain the `ff-router` binary (download or copy) and assemble a signed
/// `Firefox Router.app` inside `staging`, mirroring `scripts/package.sh`.
fn fetch_app(source: &AppSource, staging: &Path) -> io::Result<()> {
    std::fs::create_dir_all(staging)?;
    let raw = staging.join("ff-router");
    match source {
        AppSource::Local { binary } => {
            std::fs::copy(binary, &raw)?;
        }
        AppSource::Download { version } => {
            // Shell out to curl (always present on macOS) rather than pull in a
            // TLS stack; `-fsSL` fails on HTTP errors and follows the redirect
            // GitHub serves for release assets.
            check(
                Command::new("curl")
                    .args(["-fsSL", "--retry", "3", "-o"])
                    .arg(&raw)
                    .arg(download_url(version))
                    .status()?,
            )?;
        }
    }

    let bundle = staging.join(APP);
    let _ = std::fs::remove_dir_all(&bundle);
    let macos = bundle.join("Contents/MacOS");
    std::fs::create_dir_all(&macos)?;
    std::fs::write(bundle.join("Contents/Info.plist"), INFO_PLIST)?;
    let exe = macos.join("ff-router");
    std::fs::copy(&raw, &exe)?;
    set_executable(&exe)?;
    std::fs::write(bundle.join("Contents/PkgInfo"), "APPL????")?;

    // Ad-hoc sign so macOS treats the freshly-assembled bundle as valid.
    check(
        Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(&bundle)
            .status()?,
    )
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
    LOGIN_ITEM_PLIST.replace("{program}", &xml_escape(&program.display().to_string()))
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
        let source = AppSource::Download {
            version: "1.2.3".into(),
        };
        let actions = build(
            source,
            Path::new("/home/u"),
            "cfg".into(),
            Path::new("/stage"),
        );
        assert_eq!(actions.len(), 7);
        assert!(matches!(actions[0], Action::WriteFile { .. }));
        assert!(actions[0].summary().contains(".ff-router.toml"));
        assert!(matches!(actions[1], Action::FetchApp { .. }));
        assert!(
            actions[1]
                .summary()
                .contains("Download Firefox Router 1.2.3")
        );
        assert!(
            actions[1]
                .detail()
                .contains("releases/download/v1.2.3/ff-router")
        );
        assert!(matches!(actions[2], Action::MoveInto { .. }));
        assert!(actions[2].summary().contains("Firefox Router.app"));
        // The bundle is moved out of the staging dir, not the old dist/.
        assert!(actions[2].detail().contains("/stage/Firefox Router.app"));
        assert!(matches!(actions[3], Action::MakeExecutable { .. }));
        assert!(actions[3].summary().contains("chmod 755"));
        assert!(matches!(actions[4], Action::Run { .. }));
        assert!(actions[4].detail().starts_with("$ "));
        assert!(matches!(actions[5], Action::InstallLoginItem { .. }));
        assert!(actions[5].summary().contains("login"));
        assert!(actions[5].detail().contains("launchctl bootstrap"));
        assert!(matches!(actions[6], Action::Run { .. }));
        assert!(actions[6].summary().contains("default browser"));
    }

    #[test]
    fn local_source_summary_mentions_local_build() {
        let source = AppSource::Local {
            binary: PathBuf::from("/tmp/ff-router"),
        };
        let actions = build(
            source,
            Path::new("/home/u"),
            "cfg".into(),
            Path::new("/stage"),
        );
        assert!(actions[1].summary().contains("local build"));
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
