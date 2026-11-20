use boltffi::*;

use crate::{FixturePoint, FixtureStatus};

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FixtureMessageRecord {
    pub label: String,
    pub anchor: FixturePoint,
    pub status: FixtureStatus,
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FixtureStringConfig {
    pub name: String,
    pub source: String,
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FixtureMarkerOptions {
    pub id: i32,
}

/// Ordered by `major`, then a release above its pre-releases, which its fields' order
/// would not give.
#[data(ord)]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FixtureVersion {
    pub major: u32,
    pub pre: Option<String>,
}

impl Ord for FixtureVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(left), Some(right)) => left.cmp(right),
            })
    }
}

impl PartialOrd for FixtureVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[export]
pub fn describe_message(record: FixtureMessageRecord) -> String {
    format!(
        "{}:{}:{}:{:?}",
        record.label, record.anchor.x, record.anchor.y, record.status
    )
}

#[export]
pub fn peek_label(record: &FixtureMessageRecord) -> u32 {
    record.label.len() as u32
}

#[export]
pub fn relabel(record: &mut FixtureMessageRecord, label: String) {
    record.label = label;
}

#[export]
pub fn make_message(label: String) -> FixtureMessageRecord {
    FixtureMessageRecord {
        label,
        anchor: FixturePoint { x: 5.0, y: 8.0 },
        status: FixtureStatus::Active,
    }
}

#[data(impl)]
impl FixtureStringConfig {
    pub fn from_owned_name(name: String) -> Self {
        Self {
            name,
            source: "owned".to_string(),
        }
    }

    pub fn from_borrowed_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            source: "borrowed".to_string(),
        }
    }

    #[allow(clippy::ptr_arg)]
    pub fn from_string_ref_name(name: &String) -> Self {
        Self {
            name: name.clone(),
            source: "string_ref".to_string(),
        }
    }
}
