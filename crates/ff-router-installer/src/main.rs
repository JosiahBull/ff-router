//! Installer for firefox-link-router.
//!
//! By default it runs an interactive TUI: discover Firefox profiles, walk you
//! through building `~/.ff-router.toml`, then step through the install plan
//! action-by-action. Pass `--non-interactive` to run the same plan headlessly
//! against an existing (or `--config`-supplied) config.
//!
//! Run with `--help` for the full flag list.

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
use clap::Parser;
use plan::{Action, AppSource, Decided};

/// Install Firefox Router: assemble the app bundle, register it, and set up the
/// login item. Runs an interactive TUI by default; pass `--non-interactive` for
/// scripted setups.
#[derive(Debug, Parser)]
#[command(name = "ff-router-installer", version, about)]
struct Cli {
    /// Print discovered Firefox profiles and exit.
    #[arg(long)]
    list: bool,

    /// Run the install plan headlessly, without the TUI.
    #[arg(long, short = 'y', visible_alias = "yes")]
    non_interactive: bool,

    /// Config file to install as ~/.ff-router.toml. Without it, an existing
    /// ~/.ff-router.toml is reused as-is. (Non-interactive only.)
    #[arg(long, value_name = "FILE", requires = "non_interactive")]
    config: Option<PathBuf>,

    /// Skip the "become default browser" step. (Non-interactive only.)
    #[arg(long, requires = "non_interactive")]
    no_set_default: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--list` prints discovered profiles and exits (handy for debugging).
    if cli.list {
        for p in &discover::discover() {
            println!("{:<20} {:<12} {}", p.name, p.label, p.dir);
        }
        return ExitCode::SUCCESS;
    }

    // Headless install for scripted setups (dotfiles, CI). Skips the TUI and the
    // interactive-terminal requirement below; profiles are expected to exist
    // already (the caller creates them) and the config is supplied or reused.
    if cli.non_interactive {
        return run_non_interactive(cli.config.as_deref(), cli.no_set_default);
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
fn run_non_interactive(config_arg: Option<&Path>, no_set_default: bool) -> ExitCode {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        eprintln!("error: HOME is not set");
        return ExitCode::FAILURE;
    };
    let target = home.join(".ff-router.toml");

    // Decide the config text and whether the plan should (over)write the file.
    let (config_contents, write_config) = match config_arg {
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

    /// clap's own lint pass: catches conflicting shorts, bad `requires`, etc.
    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn config_accepts_both_forms_and_all_aliases() {
        for flag in ["--non-interactive", "--yes", "-y"] {
            let cli = Cli::try_parse_from(["ff-router-installer", flag]).unwrap();
            assert!(cli.non_interactive, "{flag} should set non_interactive");
        }
        // `--config value` and `--config=value` both parse to the same path.
        for form in [
            ["--config", "/a/b.toml"].as_slice(),
            ["--config=/a/b.toml"].as_slice(),
        ] {
            let argv = [&["ff-router-installer", "-y"], form].concat();
            let cli = Cli::try_parse_from(argv).unwrap();
            assert_eq!(cli.config.as_deref(), Some(Path::new("/a/b.toml")));
        }
    }

    #[test]
    fn config_requires_non_interactive() {
        // --config / --no-set-default are meaningless without --non-interactive
        // and are now rejected rather than silently ignored.
        assert!(Cli::try_parse_from(["ff-router-installer", "--config", "/a.toml"]).is_err());
        assert!(Cli::try_parse_from(["ff-router-installer", "--no-set-default"]).is_err());
    }

    #[test]
    fn unknown_flags_are_rejected() {
        assert!(Cli::try_parse_from(["ff-router-installer", "--no-set-defualt", "-y"]).is_err());
    }
}
