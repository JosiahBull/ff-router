//! Render a line-level diff as coloured ratatui lines: red for removed lines,
//! green for added lines, dim for unchanged context.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use similar::{ChangeTag, TextDiff};

pub fn lines(existing: &str, proposed: &str) -> Vec<Line<'static>> {
    let diff = TextDiff::from_lines(existing, proposed);
    let mut out = Vec::new();
    for change in diff.iter_all_changes() {
        let (sign, style) = match change.tag() {
            ChangeTag::Delete => ('-', Style::default().fg(Color::Red)),
            ChangeTag::Insert => ('+', Style::default().fg(Color::Green)),
            ChangeTag::Equal => (' ', Style::default().fg(Color::DarkGray)),
        };
        let text = change.value().trim_end_matches('\n').to_string();
        out.push(Line::from(Span::styled(format!("{sign} {text}"), style)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::lines;

    #[test]
    fn marks_added_and_removed_lines() {
        let rows = lines("a\nb\n", "a\nc\n");
        // equal "a", delete "b", insert "c"
        assert_eq!(rows.len(), 3);
        assert!(rows[0].spans[0].content.contains('a'));
        assert!(
            rows[1].spans[0].content.starts_with("- ") && rows[1].spans[0].content.contains('b')
        );
        assert!(
            rows[2].spans[0].content.starts_with("+ ") && rows[2].spans[0].content.contains('c')
        );
    }
}
