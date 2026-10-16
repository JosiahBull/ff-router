//! Translate supported web destinations without opening an intermediate browser.

use std::collections::HashMap;

use serde::Deserialize;
use url::Url;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DesktopConfig {
    linear: bool,
    notion: bool,
    slack: bool,
    /// Workspace subdomain -> team ID; no credentials or network lookup needed.
    slack_workspaces: HashMap<String, String>,
}

#[derive(Debug, PartialEq)]
pub struct Destination {
    pub bundle_id: &'static str,
    pub url: String,
}

impl DesktopConfig {
    pub fn route(&self, input: &str) -> Option<Destination> {
        let url = Url::parse(input).ok()?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            return None;
        }
        let host = url.host_str()?;
        let path: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();

        // Workspace content only: documentation, login, and marketing stay web.
        if self.linear
            && host == "linear.app"
            && path.len() >= 2
            && matches!(
                path[1],
                "issue"
                    | "project"
                    | "team"
                    | "view"
                    | "views"
                    | "projects"
                    | "initiatives"
                    | "my-issues"
                    | "inbox"
                    | "settings"
                    | "document"
                    | "cycle"
                    | "profiles"
            )
        {
            return Some(destination("com.linear", "linear", input));
        }

        if self.notion
            && (host == "notion.so"
                || host.ends_with(".notion.so")
                || host == "notion.site"
                || host.ends_with(".notion.site")
                || host == "app.notion.com")
            && path.last().is_some_and(|segment| page_id(segment))
        {
            return Some(destination("notion.id", "notion", input));
        }

        if self.slack {
            return self.slack_url(&url, &path).map(|url| Destination {
                bundle_id: "com.tinyspeck.slackmacgap",
                url,
            });
        }
        None
    }

    fn slack_url(&self, url: &Url, path: &[&str]) -> Option<String> {
        let host = url.host_str()?;
        let (team, channel, message) = if host == "app.slack.com" {
            match path {
                ["client", team] if slack_id(team, "T") => (*team, None, None),
                ["client", team, channel] if slack_id(team, "T") && slack_id(channel, "CGD") => {
                    (*team, Some(*channel), None)
                }
                ["client", team, channel, message]
                    if slack_id(team, "T") && slack_id(channel, "CGD") =>
                {
                    (*team, Some(*channel), Some(timestamp(message)?))
                }
                _ => return None,
            }
        } else {
            let domain = host.strip_suffix(".slack.com")?;
            let team = self.slack_workspaces.get(domain)?;
            if !slack_id(team, "T") {
                return None;
            }
            match path {
                [] => (team.as_str(), None, None),
                ["archives" | "messages", channel] if slack_id(channel, "CGD") => {
                    (team.as_str(), Some(*channel), None)
                }
                ["archives", channel, message] if slack_id(channel, "CGD") => {
                    (team.as_str(), Some(*channel), Some(timestamp(message)?))
                }
                _ => return None,
            }
        };

        let mut target = Url::parse(if channel.is_some() {
            "slack://channel"
        } else {
            "slack://open"
        })
        .ok()?;
        {
            let mut query = target.query_pairs_mut();
            query.append_pair("team", team);
            if let Some(channel) = channel {
                query.append_pair("id", channel);
            }
            if let Some(message) = message {
                query.append_pair("message", &message);
            }
            // Preserve thread context; permalink timestamps identify the reply.
            for (key, value) in url.query_pairs() {
                if key == "thread_ts" && timestamp(&value).is_some() {
                    query.append_pair("thread_ts", &value);
                }
            }
        }
        Some(target.into())
    }
}

fn destination(bundle_id: &'static str, scheme: &str, input: &str) -> Destination {
    Destination {
        bundle_id,
        // Preserve the path, escaped characters, query, and block fragment.
        url: format!("{scheme}:{}", input.split_once(':').unwrap().1),
    }
}

fn page_id(segment: &str) -> bool {
    let compact = segment.rsplit('-').next().unwrap_or("");
    if compact.len() == 32 && compact.bytes().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    // Hyphenated UUID, optionally following a page title.
    let Some(tail) = segment.get(segment.len().saturating_sub(36)..) else {
        return false;
    };
    tail.len() == 36
        && tail.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn slack_id(value: &str, prefixes: &str) -> bool {
    value.len() > 1
        && prefixes.contains(value.as_bytes()[0] as char)
        && value
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn timestamp(value: &str) -> Option<String> {
    if let Some(digits) = value.strip_prefix('p') {
        if digits.len() == 16 && digits.bytes().all(|c| c.is_ascii_digit()) {
            return Some(format!("{}.{}", &digits[..10], &digits[10..]));
        }
    } else if let Some((seconds, micros)) = value.split_once('.')
        && seconds.len() == 10
        && micros.len() == 6
        && seconds
            .bytes()
            .chain(micros.bytes())
            .all(|c| c.is_ascii_digit())
    {
        return Some(value.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> DesktopConfig {
        toml::from_str(
            "linear = true\nnotion = true\nslack = true\n[slack_workspaces]\nexample = 'T123ABC'",
        )
        .unwrap()
    }

    #[test]
    fn linear_preserves_issue_query_and_fragment() {
        let input = "https://linear.app/example/issue/ENG-123/title?foo=a%20b#comment-1";
        assert_eq!(
            enabled().route(input).unwrap().url,
            "linear://linear.app/example/issue/ENG-123/title?foo=a%20b#comment-1"
        );
        assert!(DesktopConfig::default().route(input).is_none());
    }

    #[test]
    fn notion_pages_keep_views_and_block_anchors() {
        for host in ["www.notion.so", "example.notion.site", "app.notion.com"] {
            let input = format!("https://{host}/p/1234567890abcdef1234567890abcdef?v=123#block");
            assert_eq!(
                enabled().route(&input).unwrap().url,
                input.replacen("https:", "notion:", 1)
            );
        }
        assert!(page_id("Title-12345678-90ab-cdef-1234-567890abcdef"));
        assert!(!page_id("ééééééééééééééééééé"));
    }

    #[test]
    fn slack_permalink_keeps_message_and_thread() {
        let target = enabled().route("https://example.slack.com/archives/C123ABC/p1723456789123456?thread_ts=1723456780.000001&cid=C123ABC").unwrap();
        assert_eq!(
            target.url,
            "slack://channel?team=T123ABC&id=C123ABC&message=1723456789.123456&thread_ts=1723456780.000001"
        );
        assert_eq!(
            enabled()
                .route("https://app.slack.com/client/T123ABC/D456DEF")
                .unwrap()
                .url,
            "slack://channel?team=T123ABC&id=D456DEF"
        );
        assert_eq!(
            enabled()
                .route("https://app.slack.com/client/T123ABC/C123ABC/thread/C123ABC-123")
                .map(|d| d.url),
            None
        );
    }

    #[test]
    fn unsupported_and_lookalike_links_stay_in_browser() {
        for input in [
            "https://linear.app/docs/get-the-app",
            "https://linear.app/login",
            "https://linear.app.evil.test/example/issue/ENG-1",
            "https://evil.test/linear.app/example/issue/ENG-1",
            "https://notion.so/help",
            "https://www.notion.com/help/notion-for-desktop",
            "https://notion.so.evil.test/1234567890abcdef1234567890abcdef",
            "https://unknown.slack.com/archives/C123ABC",
            "https://example.slack.com/admin",
            "https://slack.com/help",
            "https://example.slack.com/archives/C123ABC/p123",
            "https://example.slack.com@evil.test/archives/C123ABC",
            "https://linear.app:8443/example/issue/ENG-1",
            "file:///tmp/test",
        ] {
            assert!(enabled().route(input).is_none(), "{input}");
        }
    }
}
