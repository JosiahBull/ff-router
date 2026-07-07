//! Interactive TUI installer for firefox-link-router.
//!
//! Discovers Firefox profiles, walks you through building `~/.ff-router.toml`,
//! then steps through the install plan action-by-action. The `ff-router`
//! binary is downloaded from the matching GitHub release (or taken from a
//! local build via `FF_ROUTER_BIN`) and assembled into the app bundle as one
//! of the plan's steps — no repo checkout required.

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
use plan::AppSource;

fn main() -> ExitCode {
    let profiles = discover::discover();

    // `--list` prints discovered profiles and exits (handy for debugging).
    if std::env::args().any(|a| a == "--list") {
        for p in &profiles {
            println!("{:<20} {:<12} {}", p.name, p.label, p.dir);
        }
        return ExitCode::SUCCESS;
    }

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

/// Decide where `ff-router` comes from: a local build when `FF_ROUTER_BIN`
/// points at one (for development), otherwise the release matching this
/// installer's own version.
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
