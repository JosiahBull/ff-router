//! Installer for firefox-link-router.
//!
//! By default it runs an interactive TUI: discover Firefox profiles, walk you
//! through building `~/.ff-router.toml`, then step through the install plan
//! action-by-action. Pass `--non-interactive` to run the same plan headlessly
//! against an existing (or `--config`-supplied) config — used by scripted
//! installs such as a dotfiles bootstrap. In both modes the `ff-router` binary
//! is downloaded from the matching GitHub release (or taken from a local build
//! via `FF_ROUTER_BIN`) and assembled into the app bundle as one of the plan's
//! steps — no repo checkout required.
//!
//! Flags:
//!   --list             print discovered Firefox profiles and exit
//!   --non-interactive  run the install plan without the TUI (alias: -y, --yes)
//!   --config <file>    (non-interactive) write this file to ~/.ff-router.toml;
//!                      without it, an existing ~/.ff-router.toml is used as-is
//!   --no-set-default   (non-interactive) skip the "become default browser" step

mod app;
mod config;
mod diff;
mod discover;
mod glob_help;
mod plan;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use app::{Outcome, Wizard};
use plan::{Action, AppSource, Decided};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `--list` prints discovered profiles and exits (handy for debugging).
    if args.iter().any(|a| a == "--list") {
        for p in &discover::discover() {
            println!("{:<20} {:<12} {}", p.name, p.label, p.dir);
        }
        return ExitCode::SUCCESS;
    }

    // Headless install for scripted setups (dotfiles, CI). Skips the TUI and the
    // interactive-terminal requirement below; profiles are expected to exist
    // already (the caller creates them) and the config is supplied or reused.
    if args
        .iter()
        .any(|a| a == "--non-interactive" || a == "--yes" || a == "-y")
    {
        return run_non_interactive(&args);
    }

    let profiles = discover::discover();
    if profiles.is_empty() {
        eprintln!(
            "No Firefox profiles found under ~/Library/Application Support/Firefox/Profiles."
        );
        return ExitCode::FAILURE;
    }

    let source = match app_source() {
        Ok(source) => source,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("The installer needs an interactive terminal (run it directly, not piped).");
        eprintln!("For scripted installs, pass --non-interactive.");
        return ExitCode::FAILURE;
    }

    // Assemble the bundle in a throwaway staging dir; it's cleaned up after the
    // plan runs (the .app is moved out into ~/Applications).
    let staging = std::env::temp_dir().join(format!("ff-router-install-{}", std::process::id()));

    let mut terminal = ratatui::init();
    let outcome = Wizard::new(profiles, source, staging.clone()).run(&mut terminal);
    ratatui::restore();

    let code = match outcome {
        Ok(Outcome::Install { plan, warnings }) => execute(&plan, &warnings),
        Ok(Outcome::Cancelled) => {
            println!("Cancelled — nothing was changed.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    };
    let _ = std::fs::remove_dir_all(&staging);
    code
}

/// Decide where `ff-router` comes from.
fn app_source() -> Result<AppSource, String> {
    match std::env::var_os("FF_ROUTER_BIN") {
        Some(path) if Path::new(&path).is_file() => Ok(AppSource::Local {
            binary: PathBuf::from(path),
        }),
        Some(path) => Err(format!(
            "FF_ROUTER_BIN is set but is not a file: {}",
            path.to_string_lossy()
        )),
        None => Ok(AppSource::Download {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
    }
}

/// Run the install plan without the TUI, for scripted setups.
///
/// The config comes from `--config <file>` (written to `~/.ff-router.toml`) or,
/// absent that, the existing `~/.ff-router.toml` is installed around without
/// being rewritten. Every step is applied; `--no-set-default` drops the final
/// "become the default browser" prompt.
fn run_non_interactive(args: &[String]) -> ExitCode {
    let no_set_default = args.iter().any(|a| a == "--no-set-default");
    let config_arg = flag_value(args, "--config");

    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        eprintln!("error: HOME is not set");
        return ExitCode::FAILURE;
    };
    let target = home.join(".ff-router.toml");

    // Decide the config text and whether the plan should (over)write the file.
    let (config_contents, write_config) = match &config_arg {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(text) => (text, true),
            Err(e) => {
                eprintln!("error: cannot read --config {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => match std::fs::read_to_string(&target) {
            Ok(text) => (text, false),
            Err(_) => {
                eprintln!(
                    "error: {} not found; pass --config <file> or create it first",
                    plan::home_relative(&target)
                );
                return ExitCode::FAILURE;
            }
        },
    };

    let source = match app_source() {
        Ok(source) => source,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Preserve a config we're about to replace with different content.
    if write_config {
        if let Ok(existing) = std::fs::read_to_string(&target) {
            if existing != config_contents {
                let backup = home.join(".ff-router.toml.bak");
                match std::fs::copy(&target, &backup) {
                    Ok(_) => {
                        println!(
                            "Backed up existing config to {}",
                            plan::home_relative(&backup)
                        )
                    }
                    Err(e) => eprintln!("warning: could not back up existing config: {e}"),
                }
            }
        }
    }

    let staging = std::env::temp_dir().join(format!("ff-router-install-{}", std::process::id()));
    let mut actions = plan::build(source, &home, config_contents, &staging);
    if !write_config {
        // Reuse the existing config untouched — drop the write step.
        actions.retain(|a| !matches!(a, Action::WriteFile { .. }));
    }
    if no_set_default {
        actions.retain(|a| !a.summary().contains("default browser"));
    }

    let decided: Vec<Decided> = actions
        .into_iter()
        .map(|action| Decided {
            action,
            apply: true,
        })
        .collect();

    let code = execute(&decided, &[]);
    let _ = std::fs::remove_dir_all(&staging);
    code
}

/// The value following `name` in `args`, supporting both `--flag value` and
/// `--flag=value`.
fn flag_value(args: &[String], name: &str) -> Option<PathBuf> {
    let eq_prefix = format!("{name}=");
    for (i, arg) in args.iter().enumerate() {
        if arg == name {
            return args.get(i + 1).map(PathBuf::from);
        }
        if let Some(rest) = arg.strip_prefix(&eq_prefix) {
            return Some(PathBuf::from(rest));
        }
    }
    None
}

/// Execute the decided plan with plain-text logging (the TUI is already torn
/// down, so command output is safe to print here).
fn execute(plan: &[plan::Decided], warnings: &[String]) -> ExitCode {
    for warning in warnings {
        warn(warning);
    }

    let mut failed = false;
    for step in plan {
        if !step.apply {
            println!("• skipped: {}", step.action.summary());
            continue;
        }
        println!("→ {}", step.action.summary());
        if let Err(e) = step.action.execute() {
            failed = true;
            eprintln!("  {}{e}", red("failed: "));
        }
    }

    if failed {
        eprintln!("\nFinished with errors.");
        return ExitCode::FAILURE;
    }
    println!("\n✓ Done. Set 'Firefox Router' as your default browser:");
    println!("  System Settings > Desktop & Dock > Default web browser");
    ExitCode::SUCCESS
}

fn warn(msg: &str) {
    if std::io::stdout().is_terminal() {
        println!("\x1b[33mwarning:\x1b[0m {msg}");
    } else {
        println!("warning: {msg}");
    }
}

fn red(s: &str) -> String {
    if std::io::stderr().is_terminal() {
        format!("\x1b[31m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn flag_value_reads_separate_and_joined_forms() {
        assert_eq!(
            flag_value(&args(&["--config", "/a/b.toml"]), "--config"),
            Some(PathBuf::from("/a/b.toml"))
        );
        assert_eq!(
            flag_value(&args(&["--config=/a/b.toml"]), "--config"),
            Some(PathBuf::from("/a/b.toml"))
        );
    }

    #[test]
    fn flag_value_is_none_when_absent_or_dangling() {
        assert_eq!(flag_value(&args(&["--non-interactive"]), "--config"), None);
        // Flag present as the last arg with no following value.
        assert_eq!(flag_value(&args(&["--config"]), "--config"), None);
    }
}
