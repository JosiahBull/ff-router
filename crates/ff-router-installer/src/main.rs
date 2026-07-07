//! Interactive TUI installer for firefox-link-router.
//!
//! Discovers Firefox profiles, walks you through building `~/.ff-router.toml`,
//! then builds, bundles, and installs the app — removing its own build
//! artifacts when it is done.

mod app;
mod config;
mod discover;
mod install;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use app::{Outcome, Wizard};

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
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("The installer needs an interactive terminal (run it directly, not piped).");
        return ExitCode::FAILURE;
    }

    let mut terminal = ratatui::init();
    let outcome = Wizard::new(profiles).run(&mut terminal);
    ratatui::restore();

    match outcome {
        Ok(Outcome::Install(config)) => match install::run(&repo_root(), &config) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("install failed: {e}");
                ExitCode::FAILURE
            }
        },
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
