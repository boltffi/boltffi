use crate::core::{Error, Result};

const SUPPORTED_FRAMEWORKS: &str = "net5.0 or newer, netcoreapp3.0, netcoreapp3.1, netstandard2.0, netstandard2.1, or net461 or newer";

const NETFX46_FRAMEWORKS: &[&str] = &[
    "net461",
    "net462",
    "net47",
    "net471",
    "net472",
    "net48",
    "net481",
    "netstandard2.0",
];

/// Runtime the generated C# has to compile against.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CSharpProfile {
    /// .NET 5 and newer, .NET Core 3.x and .NET Standard 2.1.
    #[default]
    Modern,
    /// .NET Framework 4.6.1 and newer, and .NET Standard 2.0, with the
    /// `System.Memory`, `System.Threading.Tasks.Extensions` and
    /// `Microsoft.Bcl.AsyncInterfaces` packages.
    NetFx46,
}

/// Capabilities of the runtime that generated C# may rely on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CSharpFeatures {
    /// `Marshal.PtrToStringUTF8` is available.
    pub marshal_utf8_strings: bool,
    /// `BitConverter.SingleToInt32Bits` and `Int32BitsToSingle` are available.
    pub bit_converter_single_bits: bool,
    /// `DateTime.UnixEpoch` is available.
    pub unix_epoch: bool,
    /// `Guid` converts to and from big-endian bytes through `Span<byte>`.
    pub guid_big_endian: bool,
    /// `Interlocked.Exchange` accepts `ref ulong`.
    pub interlocked_ulong: bool,
    /// `System.Runtime.CompilerServices.IsExternalInit` ships with the runtime.
    pub is_external_init: bool,
    /// `System.Runtime.CompilerServices.OverloadResolutionPriorityAttribute` ships with the runtime.
    pub overload_resolution_priority: bool,
}

impl CSharpFeatures {
    /// Returns whether generated code has to ship stand-ins for runtime types.
    pub const fn needs_polyfills(self) -> bool {
        !self.is_external_init || !self.overload_resolution_priority
    }
}

impl CSharpProfile {
    /// Selects the profile for a target framework moniker.
    pub fn from_target_framework(framework: &str) -> Result<Self> {
        let unsupported = || Error::CSharpTargetFramework {
            framework: framework.to_owned(),
            supported: SUPPORTED_FRAMEWORKS,
        };
        let moniker = framework.trim().to_ascii_lowercase();
        let moniker = moniker.split('-').next().unwrap_or_default();
        if NETFX46_FRAMEWORKS.contains(&moniker) {
            return Ok(Self::NetFx46);
        }
        if matches!(
            moniker,
            "netcoreapp3.0" | "netcoreapp3.1" | "netstandard2.1"
        ) {
            return Ok(Self::Modern);
        }
        let version = moniker.strip_prefix("net").ok_or_else(unsupported)?;
        let (major, minor) = version.split_once('.').ok_or_else(unsupported)?;
        let major = major.parse::<u32>().map_err(|_| unsupported())?;
        minor.parse::<u32>().map_err(|_| unsupported())?;
        if major >= 5 {
            Ok(Self::Modern)
        } else {
            Err(unsupported())
        }
    }

    /// Returns the capabilities generated code may rely on under this profile.
    pub const fn features(self) -> CSharpFeatures {
        match self {
            Self::Modern => CSharpFeatures {
                marshal_utf8_strings: true,
                bit_converter_single_bits: true,
                unix_epoch: true,
                guid_big_endian: true,
                interlocked_ulong: true,
                is_external_init: true,
                overload_resolution_priority: true,
            },
            Self::NetFx46 => CSharpFeatures {
                marshal_utf8_strings: false,
                bit_converter_single_bits: false,
                unix_epoch: false,
                guid_big_endian: false,
                interlocked_ulong: false,
                is_external_init: false,
                overload_resolution_priority: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CSharpProfile;

    #[test]
    fn modern_frameworks_select_the_modern_profile() {
        for framework in [
            "net5.0",
            "net8.0",
            "net10.0",
            "net11.0",
            "net8.0-windows",
            "net8.0-windows10.0.19041",
            "NET8.0",
            "netcoreapp3.0",
            "netcoreapp3.1",
            "netstandard2.1",
        ] {
            assert_eq!(
                CSharpProfile::from_target_framework(framework),
                Ok(CSharpProfile::Modern),
                "{framework}"
            );
        }
    }

    #[test]
    fn framework_4_6_1_and_newer_select_the_netfx46_profile() {
        for framework in [
            "net461",
            "net462",
            "net47",
            "net471",
            "net472",
            "net48",
            "net481",
            "netstandard2.0",
            " net48 ",
        ] {
            assert_eq!(
                CSharpProfile::from_target_framework(framework),
                Ok(CSharpProfile::NetFx46),
                "{framework}"
            );
        }
    }

    #[test]
    fn older_and_unknown_frameworks_are_rejected() {
        for framework in [
            "",
            "net",
            "net46",
            "net40",
            "net35",
            "net4.8",
            "net4",
            "netcoreapp2.1",
            "netcoreapp3",
            "netstandard1.6",
            "net8",
            "net8.x",
            "uap10.0",
            "monoandroid13.0",
            "net8.0;net48",
        ] {
            let error = CSharpProfile::from_target_framework(framework).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("targets.csharp.target_framework"),
                "{framework}"
            );
        }
    }

    #[test]
    fn only_the_modern_profile_has_modern_apis() {
        let modern = CSharpProfile::Modern.features();
        let legacy = CSharpProfile::NetFx46.features();
        assert!(modern.marshal_utf8_strings && modern.bit_converter_single_bits);
        assert!(modern.is_external_init && !legacy.is_external_init);
        assert!(modern.overload_resolution_priority && !legacy.overload_resolution_priority);
        assert!(!legacy.marshal_utf8_strings && !legacy.bit_converter_single_bits);
        assert!(modern.unix_epoch && modern.guid_big_endian && modern.interlocked_ulong);
        assert!(!legacy.unix_epoch && !legacy.guid_big_endian && !legacy.interlocked_ulong);
    }
}
