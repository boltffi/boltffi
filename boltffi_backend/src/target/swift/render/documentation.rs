use std::fmt;

use boltffi_binding::DocComment;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Documentation {
    source: String,
}

impl Documentation {
    pub fn new(doc: Option<&DocComment>, indent: &'static str) -> Self {
        let source = doc
            .map(|doc| {
                doc.as_str()
                    .lines()
                    .map(|line| match line.is_empty() {
                        true => format!("{indent}///\n"),
                        false => format!("{indent}/// {line}\n"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { source }
    }
}

impl fmt::Display for Documentation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.source)
    }
}
