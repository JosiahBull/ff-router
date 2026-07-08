//! Optional debug logging (`debug = true` in the config). Appends one line per
//! opened URL to `~/.ff-router.log`, recording what the OS handed us (the URL
//! and the opening application) and which rule the config matched.

use std::io::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Opener;

/// Append one routing line to `~/.ff-router.log`. Best-effort: every error is
/// swallowed so debug logging can never interfere with opening a link.
pub fn log(url: &str, opener: Option<&Opener>, explanation: &str) {
    let Some(path) = crate::config::home().map(|home| home.join(".ff-router.log")) else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!(
        "{ts} url={url} opener={opener} {explanation}\n",
        ts = timestamp(now),
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

/// Format Unix `epoch_secs` (UTC) as `YYYY-MM-DDThh:mm:ssZ`. Dependency-free —
/// Howard Hinnant's civil-from-days, correct for all realistic dates.
fn timestamp(epoch_secs: u64) -> String {
    let (sec, min, hour) = (
        epoch_secs % 60,
        epoch_secs / 60 % 60,
        epoch_secs / 3_600 % 24,
    );
    let days = (epoch_secs / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

#[cfg(test)]
mod tests {
    use super::{describe, timestamp};
    use crate::Opener;

    #[test]
    fn timestamp_formats_known_epochs() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(86_400), "1970-01-02T00:00:00Z");
        assert_eq!(timestamp(1_704_067_200), "2024-01-01T00:00:00Z");
        assert_eq!(timestamp(1_704_070_923), "2024-01-01T01:02:03Z");
        // A leap day, to exercise the civil-date arithmetic.
        assert_eq!(timestamp(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn describe_covers_known_partial_and_unknown_openers() {
        let full = Opener {
            bundle_id: Some("com.tinyspeck.slackmacgap".into()),
            name: Some("Slack".into()),
        };
        assert_eq!(describe(Some(&full)), "Slack (com.tinyspeck.slackmacgap)");

        let no_name = Opener {
            bundle_id: Some("com.example".into()),
            name: None,
        };
        assert_eq!(describe(Some(&no_name)), "? (com.example)");

        assert_eq!(describe(None), "unknown");
    }
}
