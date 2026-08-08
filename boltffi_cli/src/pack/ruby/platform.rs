pub use crate::config::targets::ruby::RubyGemPlatform;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubyPackTarget {
    pub platform: RubyGemPlatform,
    pub rubygems_platform: String,
    pub rust_target: &'static str,
    pub zigbuild_target: String,
}

impl RubyPackTarget {
    pub fn from_platform(platform: RubyGemPlatform, glibc_version: Option<&str>) -> Self {
        match platform {
            RubyGemPlatform::Current => {
                let (local_gem_platform, rust_target) =
                    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                        ("arm64-darwin".to_string(), "aarch64-apple-darwin")
                    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
                        ("x86_64-darwin".to_string(), "x86_64-apple-darwin")
                    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
                        ("x86_64-linux-gnu".to_string(), "x86_64-unknown-linux-gnu")
                    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
                        ("aarch64-linux-gnu".to_string(), "aarch64-unknown-linux-gnu")
                    } else {
                        (resolve_local_ruby_platform(), "x86_64-unknown-linux-gnu")
                    };

                let zigbuild_target = if rust_target.contains("linux-gnu") {
                    if let Some(glibc) = glibc_version {
                        format!("{}.{}", rust_target, glibc)
                    } else {
                        rust_target.to_string()
                    }
                } else {
                    rust_target.to_string()
                };

                Self {
                    platform,
                    rubygems_platform: local_gem_platform,
                    rust_target,
                    zigbuild_target,
                }
            }
            RubyGemPlatform::Arm64Darwin => Self {
                platform,
                rubygems_platform: "arm64-darwin".to_string(),
                rust_target: "aarch64-apple-darwin",
                zigbuild_target: "aarch64-apple-darwin".to_string(),
            },
            RubyGemPlatform::X86_64Darwin => Self {
                platform,
                rubygems_platform: "x86_64-darwin".to_string(),
                rust_target: "x86_64-apple-darwin",
                zigbuild_target: "x86_64-apple-darwin".to_string(),
            },
            RubyGemPlatform::X86_64LinuxGnu => {
                let glibc = glibc_version.unwrap_or("2.17");
                Self {
                    platform,
                    rubygems_platform: "x86_64-linux-gnu".to_string(),
                    rust_target: "x86_64-unknown-linux-gnu",
                    zigbuild_target: format!("x86_64-unknown-linux-gnu.{}", glibc),
                }
            }
            RubyGemPlatform::Aarch64LinuxGnu => {
                let glibc = glibc_version.unwrap_or("2.17");
                Self {
                    platform,
                    rubygems_platform: "aarch64-linux-gnu".to_string(),
                    rust_target: "aarch64-unknown-linux-gnu",
                    zigbuild_target: format!("aarch64-unknown-linux-gnu.{}", glibc),
                }
            }
            RubyGemPlatform::X86_64LinuxMusl => Self {
                platform,
                rubygems_platform: "x86_64-linux-musl".to_string(),
                rust_target: "x86_64-unknown-linux-musl",
                zigbuild_target: "x86_64-unknown-linux-musl".to_string(),
            },
            RubyGemPlatform::Aarch64LinuxMusl => Self {
                platform,
                rubygems_platform: "aarch64-linux-musl".to_string(),
                rust_target: "aarch64-unknown-linux-musl",
                zigbuild_target: "aarch64-unknown-linux-musl".to_string(),
            },
        }
    }
}

fn resolve_local_ruby_platform() -> String {
    if let Ok(output) = std::process::Command::new("ruby")
        .args(["-rrubygems", "-e", "print Gem::Platform.local.to_s"])
        .output()
        && output.status.success()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    if let Some(host) = crate::target::NativeHostPlatform::current() {
        match host {
            crate::target::NativeHostPlatform::DarwinArm64 => "arm64-darwin".to_string(),
            crate::target::NativeHostPlatform::DarwinX86_64 => "x86_64-darwin".to_string(),
            crate::target::NativeHostPlatform::LinuxX86_64 => "x86_64-linux-gnu".to_string(),
            crate::target::NativeHostPlatform::LinuxAarch64 => "aarch64-linux-gnu".to_string(),
            crate::target::NativeHostPlatform::WindowsX86_64 => "x64-mingw-ucrt".to_string(),
        }
    } else {
        "current".to_string()
    }
}
