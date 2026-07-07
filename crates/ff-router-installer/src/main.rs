//! Interactive TUI installer for firefox-link-router.
//!
//! Discovers Firefox profiles, walks you through building `~/.ff-router.toml`,
//! then steps through the install plan action-by-action. The app bundle is
//! built up front by `scripts/install.sh` before this runs.

mod app;
mod config;
mod diff;
mod discover;
mod glob_help;
mod plan;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use app::{Outcome, Wizard};

const APP_BUNDLE: &str = "Firefox Router.app";

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

    let root = repo_root();
    if !root.join("dist").join(APP_BUNDLE).exists() {
        eprintln!(
            "The app bundle isn't built. Run ./scripts/install.sh (it builds everything first)."
        );
        return ExitCode::FAILURE;
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("The installer needs an interactive terminal (run it directly, not piped).");
        return ExitCode::FAILURE;
    }

    let mut terminal = ratatui::init();
    let outcome = Wizard::new(profiles, root).run(&mut terminal);
    ratatui::restore();

    match outcome {
        Ok(Outcome::Install { plan, warnings }) => execute(&plan, &warnings),
        Ok(Outcome::Cancelled) => {
            println!("Cancelled — nothing was changed.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
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

/// Locate the repo root by walking up from the executable until we find the
/// packaging script; fall back to the current directory.
fn repo_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            if ancestor.join("scripts/package.sh").is_file() {
                return ancestor.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
