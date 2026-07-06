use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFixture {
    fragments: Vec<String>,
}

impl SourceFixture {
    pub fn one(fragment: impl Into<String>) -> Self {
        Self {
            fragments: vec![fragment.into()],
        }
    }

    pub fn many<I, F>(fragments: I) -> Self
    where
        I: IntoIterator<Item = F>,
        F: Into<String>,
    {
        Self {
            fragments: fragments.into_iter().map(Into::into).collect(),
        }
    }

    pub fn read(&self) -> String {
        self.fragments
            .iter()
            .map(|fragment| fs::read_to_string(Self::path(fragment)).expect("source fixture"))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn path(fragment: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("source")
            .join(format!("{fragment}.rs"))
    }
}
