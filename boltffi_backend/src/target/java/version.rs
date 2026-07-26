use crate::core::{Error, Result};

/// A requested Java source and runtime release.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JavaVersion(pub u8);

impl Default for JavaVersion {
    fn default() -> Self {
        Self::JAVA_8
    }
}

impl JavaVersion {
    /// Java 8.
    pub const JAVA_8: Self = Self(8);
    /// Java 9.
    pub const JAVA_9: Self = Self(9);
    /// Java 11.
    pub const JAVA_11: Self = Self(11);
    /// Java 16.
    pub const JAVA_16: Self = Self(16);
    /// Java 17.
    pub const JAVA_17: Self = Self(17);
    /// Java 21.
    pub const JAVA_21: Self = Self(21);
    /// Java 22.
    pub const JAVA_22: Self = Self(22);
    /// Java 23.
    pub const JAVA_23: Self = Self(23);
    /// Java 24.
    pub const JAVA_24: Self = Self(24);
    /// Java 25.
    pub const JAVA_25: Self = Self(25);
    /// Java 26.
    pub const JAVA_26: Self = Self(26);

    /// Creates a supported Java release value.
    pub const fn new(release: u8) -> Option<Self> {
        match release >= Self::JAVA_8.0 && release <= Self::JAVA_26.0 {
            true => Some(Self(release)),
            false => None,
        }
    }

    /// Returns the Java release number.
    pub const fn release(self) -> u8 {
        self.0
    }

    /// Returns whether the release includes the Flow API.
    pub const fn supports_flow_api(&self) -> bool {
        self.0 >= 9
    }

    /// Returns whether the release includes record classes.
    pub const fn supports_records(&self) -> bool {
        self.0 >= 16
    }

    /// Returns whether the release includes sealed classes.
    pub const fn supports_sealed(&self) -> bool {
        self.0 >= 17
    }

    /// Returns whether the release includes virtual threads.
    pub const fn supports_virtual_threads(&self) -> bool {
        self.0 >= 21
    }

    /// Returns whether the release includes Cleaner.
    pub const fn supports_cleaner(&self) -> bool {
        self.0 >= 9
    }

    /// Validates the Java release range.
    pub fn validate(self) -> Result<()> {
        if Self::new(self.0).is_some() {
            Ok(())
        } else {
            Err(Error::UnsupportedTarget {
                target: "java",
                shape: "Java release outside the supported range",
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JavaVersion;

    #[test]
    fn defaults_to_java_eight() {
        assert_eq!(JavaVersion::default(), JavaVersion::JAVA_8);
    }

    #[test]
    fn bounds_supported_releases() {
        assert_eq!(JavaVersion::new(7), None);
        assert_eq!(JavaVersion::new(8), Some(JavaVersion::JAVA_8));
        assert_eq!(JavaVersion::new(27), None);
        assert_eq!(JavaVersion::new(17).unwrap().release(), 17);
    }

    #[test]
    fn preserves_legacy_tuple_construction_and_feature_queries() {
        let version = JavaVersion(17);

        assert_eq!(version.0, 17);
        assert!(version.supports_flow_api());
        assert!(version.supports_records());
        assert!(version.supports_sealed());
        assert!(!version.supports_virtual_threads());
        assert!(version.supports_cleaner());
    }

    #[test]
    fn rejects_tuple_constructed_releases_outside_the_supported_range() {
        assert!(JavaVersion(7).validate().is_err());
        assert!(JavaVersion(27).validate().is_err());
    }
}
