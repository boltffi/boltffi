use std::fmt;

use boltffi_binding::DocComment;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Documentation(String);

impl Documentation {
    pub(super) fn summary(doc: Option<&DocComment>, indent: &str) -> Self {
        let Some(doc) = normalized(doc) else {
            return Self::default();
        };
        let mut source = String::new();
        push_doc_line(&mut source, indent, "<summary>");
        push_text(&mut source, indent, &doc);
        push_doc_line(&mut source, indent, "</summary>");
        Self(source)
    }

    pub(super) fn parameter(doc: Option<&DocComment>, name: &str, indent: &str) -> Self {
        let Some(doc) = normalized(doc) else {
            return Self::default();
        };
        let lines = doc.lines().collect::<Vec<_>>();
        let mut source = String::new();
        match lines.as_slice() {
            [line] => push_doc_line(
                &mut source,
                indent,
                &format!("<param name=\"{name}\">{line}</param>"),
            ),
            _ => {
                push_doc_line(&mut source, indent, &format!("<param name=\"{name}\">"));
                push_text(&mut source, indent, &doc);
                push_doc_line(&mut source, indent, "</param>");
            }
        }
        Self(source)
    }
}

impl fmt::Display for Documentation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn normalized(doc: Option<&DocComment>) -> Option<String> {
    let doc = doc?.as_str();
    (!doc.trim().is_empty()).then(|| xml_escape(doc))
}

fn xml_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            character => escaped.push(character),
        }
    }
    escaped
}

fn push_doc_line(source: &mut String, indent: &str, text: &str) {
    source.push_str(indent);
    source.push_str("/// ");
    source.push_str(text);
    source.push('\n');
}

fn push_text(source: &mut String, indent: &str, text: &str) {
    for line in text.lines() {
        source.push_str(indent);
        source.push_str("///");
        if !line.is_empty() {
            source.push(' ');
            source.push_str(line);
        }
        source.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use boltffi_binding::DocComment;

    use super::Documentation;

    #[test]
    fn summary_escapes_xml_and_preserves_paragraphs() {
        let doc = DocComment::new("Wraps Vec<T> & friends.\n\nSecond paragraph.");

        assert_eq!(
            Documentation::summary(Some(&doc), "    ").to_string(),
            "    /// <summary>\n    /// Wraps Vec&lt;T&gt; &amp; friends.\n    ///\n    /// Second paragraph.\n    /// </summary>\n"
        );
    }

    #[test]
    fn parameter_uses_compact_single_line_form() {
        let doc = DocComment::new("The display name.");

        assert_eq!(
            Documentation::parameter(Some(&doc), "Name", "    ").to_string(),
            "    /// <param name=\"Name\">The display name.</param>\n"
        );
    }
}
