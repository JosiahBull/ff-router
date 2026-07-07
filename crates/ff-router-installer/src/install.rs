//! Write the config, build/bundle/install the app, then remove our own build
//! artifacts. Reuses the tested shell scripts for the build + bundle step.

use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

const APP: &str = "Firefox Router.app";
const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";

pub fn run(root: &Path, config: &str, warnings: &[String]) -> io::Result<()> {
    let home = home()?;

    // 1. Write ~/.ff-router.toml, backing up any existing file.
    let cfg_path = home.join(".ff-router.toml");
    if cfg_path.exists() {
        let backup = home.join(".ff-router.toml.bak");
        std::fs::copy(&cfg_path, &backup)?;
        println!("Backed up existing config to {}", backup.display());
    }
    std::fs::write(&cfg_path, config)?;
    println!("Wrote {}", cfg_path.display());

    for warning in warnings {
        warn(warning);
    }

    // 2. Build the optimised binary and assemble the signed bundle.
    println!("\nBuilding the optimised binary and app bundle (this can take a minute)…");
    ok(Command::new("bash")
        .arg("scripts/package.sh")
        .current_dir(root)
        .status()?)?;

    // 3. Install into ~/Applications.
    let dest_dir = home.join("Applications");
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(APP);
    let _ = std::fs::remove_dir_all(&dest);
    ok(Command::new("cp")
        .arg("-R")
        .arg(root.join("dist").join(APP))
        .arg(&dest)
        .status()?)?;
    println!("Installed {}", dest.display());

    // 4. Register with Launch Services.
    let _ = Command::new(LSREGISTER).arg("-f").arg(&dest).status();

    // 5. Clean up after ourselves: the heavy installer + build artifacts.
    println!("\nCleaning up build artifacts…");
    let _ = std::fs::remove_dir_all(root.join("dist"));
    let _ = Command::new("cargo")
        .arg("clean")
        .current_dir(root)
        .status();

    println!("\n✓ Installed. Set 'Firefox Router' as your default browser:");
    println!("  System Settings > Desktop & Dock > Default web browser");
    Ok(())
}

/// Print a warning, in yellow when writing to a terminal.
fn warn(msg: &str) {
    if io::stdout().is_terminal() {
        println!("\x1b[33mwarning:\x1b[0m {msg}");
    } else {
        println!("warning: {msg}");
    }
}

fn home() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("HOME is not set"))
}

fn ok(status: ExitStatus) -> io::Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("command failed ({status})")))
    }
}
