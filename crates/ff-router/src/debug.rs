//! Optional debug logging (`debug = true` in the config). Appends one line per
//! opened URL to `~/.ff-router.log`, recording what the OS handed us (the URL
//! and the opening application) and which rule the config matched.

use std::io::Write as _;

use crate::Opener;

/// Append one routing line to `~/.ff-router.log`.
pub fn log(url: &str, opener: Option<&Opener>, explanation: &str) {
    let Some(path) = crate::config::home().map(|home| home.join(".ff-router.log")) else {
        return;
    };

    let line = format!(
        "{ts} url={url} opener={opener} {explanation}\n",
        ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        opener = describe(opener),
    );
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// `Name (bundle.id)` for a known opener (with `?` for a missing field), else
/// `unknown` when the OS attached no sender.
fn describe(opener: Option<&Opener>) -> String {
    match opener {
        None => "unknown".to_string(),
        Some(o) => format!(
            "{} ({})",
            o.name.as_deref().unwrap_or("?"),
            o.bundle_id.as_deref().unwrap_or("?"),
        ),
    }
}
