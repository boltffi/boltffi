mod tables;

use std::cmp::Ordering;

use crate::target::java::JavaVersion;

use tables::{IDENTIFIER_IGNORABLE, IDENTIFIER_PART, IDENTIFIER_START, UPGRADE_RELEASES};

const _: [(); tables::MIN_RELEASE as usize] = [(); JavaVersion::JAVA_8.release() as usize];
const _: [(); tables::MAX_RELEASE as usize] = [(); JavaVersion::JAVA_26.release() as usize];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JavaIdentifiers {
    upgrades: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CharacterSet(&'static [(u32, u32)]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CharacterChanges {
    additions: CharacterSet,
    removals: CharacterSet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VersionedCharacterSet {
    base: CharacterSet,
    changes: &'static [CharacterChanges],
}

impl JavaIdentifiers {
    pub fn for_version(version: JavaVersion) -> Self {
        Self {
            upgrades: UPGRADE_RELEASES.partition_point(|release| *release <= version.release()),
        }
    }

    pub fn start(self, character: char) -> bool {
        IDENTIFIER_START.contains(character, self.upgrades)
    }

    pub fn part(self, character: char) -> bool {
        IDENTIFIER_PART.contains(character, self.upgrades)
    }

    pub fn ignorable(self, character: char) -> bool {
        IDENTIFIER_IGNORABLE.contains(character, self.upgrades)
    }
}

impl CharacterSet {
    fn contains(self, codepoint: u32) -> bool {
        self.0
            .binary_search_by(|(start, end)| {
                if *end < codepoint {
                    Ordering::Less
                } else if *start > codepoint {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .is_ok()
    }

    #[cfg(test)]
    fn is_ordered(self) -> bool {
        self.0.windows(2).all(|ranges| ranges[0].1 < ranges[1].0)
    }
}

impl CharacterChanges {
    fn apply(self, codepoint: u32, member: bool) -> bool {
        if self.additions.contains(codepoint) {
            true
        } else if self.removals.contains(codepoint) {
            false
        } else {
            member
        }
    }
}

impl VersionedCharacterSet {
    fn contains(self, character: char, upgrades: usize) -> bool {
        let codepoint = u32::from(character);
        self.changes
            .iter()
            .take(upgrades)
            .fold(self.base.contains(codepoint), |member, changes| {
                changes.apply(codepoint, member)
            })
    }

    #[cfg(test)]
    fn is_ordered(self) -> bool {
        self.base.is_ordered()
            && self
                .changes
                .iter()
                .all(|changes| changes.additions.is_ordered() && changes.removals.is_ordered())
    }
}

#[cfg(test)]
mod tests {
    use crate::target::java::JavaVersion;

    use super::{IDENTIFIER_IGNORABLE, IDENTIFIER_PART, IDENTIFIER_START, JavaIdentifiers};

    #[test]
    fn keeps_versioned_character_ranges_ordered() {
        assert!(
            [IDENTIFIER_START, IDENTIFIER_PART, IDENTIFIER_IGNORABLE]
                .into_iter()
                .all(|characters| characters.is_ordered())
        );
    }

    #[test]
    fn follows_java_release_unicode_upgrades() {
        let java_8 = JavaIdentifiers::for_version(JavaVersion::JAVA_8);
        let java_9 = JavaIdentifiers::for_version(JavaVersion::JAVA_9);
        let java_11 = JavaIdentifiers::for_version(JavaVersion::JAVA_11);
        let java_17 = JavaIdentifiers::for_version(JavaVersion::JAVA_17);
        let java_19 = JavaIdentifiers::for_version(JavaVersion::new(19).unwrap());
        let java_20 = JavaIdentifiers::for_version(JavaVersion::new(20).unwrap());
        let java_21 = JavaIdentifiers::for_version(JavaVersion::JAVA_21);
        let java_22 = JavaIdentifiers::for_version(JavaVersion::JAVA_22);
        let java_23 = JavaIdentifiers::for_version(JavaVersion::JAVA_23);
        let java_24 = JavaIdentifiers::for_version(JavaVersion::JAVA_24);
        let java_25 = JavaIdentifiers::for_version(JavaVersion::JAVA_25);
        let java_26 = JavaIdentifiers::for_version(JavaVersion::JAVA_26);

        assert!(java_8.start('\u{1885}'));
        assert!(java_9.start('\u{1885}'));
        assert!(!java_11.start('\u{1885}'));
        assert!(!java_9.start('\u{1e900}'));
        assert!(java_11.start('\u{1e900}'));
        assert!(!java_19.start('\u{11f02}'));
        assert!(java_20.start('\u{11f02}'));
        assert!(!java_21.start('\u{2ebf0}'));
        assert!(java_22.start('\u{2ebf0}'));
        assert!(java_23.start('\u{2ebf0}'));
        assert!(!java_23.start('\u{11380}'));
        assert!(java_24.start('\u{11380}'));
        assert!(java_25.start('\u{11380}'));
        assert!(!java_25.start('\u{10940}'));
        assert!(java_26.start('\u{10940}'));
        assert!(!java_17.start('\u{1885}'));
    }
}
