#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Platform {
    operating_systems: &'static [&'static str],
    architectures: &'static [&'static str],
    directories: &'static [&'static str],
}

impl Platform {
    pub const fn new(
        operating_systems: &'static [&'static str],
        architectures: &'static [&'static str],
        directories: &'static [&'static str],
    ) -> Self {
        Self {
            operating_systems,
            architectures,
            directories,
        }
    }

    pub const fn operating_systems(&self) -> &'static [&'static str] {
        self.operating_systems
    }

    pub const fn architectures(&self) -> &'static [&'static str] {
        self.architectures
    }

    pub const fn directories(&self) -> &'static [&'static str] {
        self.directories
    }
}

pub const PLATFORMS: &[Platform] = &[
    Platform::new(
        &["mac", "darwin"],
        &["aarch64", "arm64"],
        &["darwin-arm64", "darwin-aarch64"],
    ),
    Platform::new(
        &["mac", "darwin"],
        &["x86_64", "amd64"],
        &["darwin-x86_64", "darwin-x86-64"],
    ),
    Platform::new(
        &["linux"],
        &["x86_64", "amd64"],
        &["linux-x86_64", "linux-x86-64"],
    ),
    Platform::new(
        &["linux"],
        &["aarch64", "arm64"],
        &["linux-aarch64", "linux-arm64"],
    ),
    Platform::new(
        &["windows"],
        &["x86_64", "amd64"],
        &["windows-x86_64", "windows-x86-64", "win32-x86_64"],
    ),
    Platform::new(
        &["windows"],
        &["aarch64", "arm64"],
        &["windows-aarch64", "windows-arm64", "win32-arm64"],
    ),
];

#[cfg(test)]
mod tests {
    use super::PLATFORMS;

    #[test]
    fn includes_windows_arm64_resource_candidates() {
        let windows_arm64 = PLATFORMS
            .iter()
            .find(|platform| {
                platform.operating_systems().contains(&"windows")
                    && platform.architectures().contains(&"arm64")
            })
            .unwrap();

        assert_eq!(
            windows_arm64.directories(),
            &["windows-aarch64", "windows-arm64", "win32-arm64"]
        );
    }
}
