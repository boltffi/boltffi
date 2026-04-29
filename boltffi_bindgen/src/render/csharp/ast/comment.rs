//! C# doc comment text. Wraps the raw doc string lifted from a Rust
//! `///` comment; templates render the surrounding `///` framing
//! themselves so they own the indent and the doc-tag shape (`<summary>`
//! today, `<param>` and friends later).

use std::fmt;

/// The text of a doc comment, with XML special characters escaped.
/// Lines are exposed via [`CSharpComment::lines`] for templates to
/// walk under their own `///` prefix. Construct via
/// [`CSharpComment::from_str_option`]; whitespace-only or absent
/// input collapses to `None` so plan fields use
/// `Option<CSharpComment>` to gate the doc block on/off.
///
/// Examples:
/// ```csharp
/// /// <summary>
/// /// A point in 2D space.
/// /// </summary>
/// public readonly record struct Point(double X, double Y);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CSharpComment(String);

impl CSharpComment {
    /// Wraps an optional doc string from the IR, escaping `&`, `<`,
    /// and `>` so the rendered comment is well-formed XML. Returns
    /// `None` when the input is `None` or whitespace-only, so plan
    /// fields fall through to no doc block at all.
    pub(crate) fn from_str_option(opt: Option<&str>) -> Option<Self> {
        let text = opt?;
        if text.trim().is_empty() {
            return None;
        }
        Some(Self(xml_escape(text)))
    }

    /// Walks the escaped doc text line-by-line. Templates emit each
    /// line under a `///` prefix at their own indent; empty lines
    /// produce a bare `///` (paragraph separator).
    pub(crate) fn lines(&self) -> std::str::Lines<'_> {
        self.0.lines()
    }
}

impl fmt::Display for CSharpComment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `None` from the IR side maps to `None`; the lower step writes
    /// `CSharpComment::from_str_option(record.doc.as_deref())`.
    #[test]
    fn from_str_option_returns_none_when_input_is_none() {
        assert!(CSharpComment::from_str_option(None).is_none());
    }

    /// Whitespace-only doc is treated as absent — Rust accepts a blank
    /// `///`, but rendering an empty doc block produces noise.
    #[test]
    fn from_str_option_returns_none_for_whitespace_only() {
        assert!(CSharpComment::from_str_option(Some("   \n   ")).is_none());
    }

    /// Special XML characters are escaped at construction so callers
    /// (templates included) never need to know about escaping.
    #[test]
    fn from_str_option_escapes_xml_special_characters() {
        let comment = CSharpComment::from_str_option(Some("Wraps Vec<T> & co.")).unwrap();
        assert_eq!(comment.to_string(), "Wraps Vec&lt;T&gt; &amp; co.");
    }

    /// Multi-line doc surfaces each line for templates to walk.
    /// Blank lines are preserved as empty entries, since paragraph
    /// breaks render as bare `///` lines in the surrounding doc block.
    #[test]
    fn lines_yields_each_line_including_blank_separators() {
        let comment = CSharpComment::from_str_option(Some("First.\n\nSecond.")).unwrap();
        let collected: Vec<&str> = comment.lines().collect();
        assert_eq!(collected, vec!["First.", "", "Second."]);
    }
}
