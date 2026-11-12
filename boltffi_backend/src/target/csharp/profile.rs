use crate::core::{Error, Result};

const SUPPORTED_FRAMEWORKS: &str = "net5.0 or newer, netcoreapp3.0, netcoreapp3.1, netstandard2.0, netstandard2.1, net461 or newer, net45, net451, net452, or net46";

const NETFX45_FRAMEWORKS: &[&str] = &["net45", "net451", "net452", "net46"];

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
    /// .NET Framework 4.5 through 4.6, with `System.ValueTuple`. Async
    /// streams are not available.
    NetFx45,
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
    /// `Span<T>`, `MemoryMarshal`, `BinaryPrimitives` and `Unsafe` are available.
    pub system_memory: bool,
    /// The generic `Marshal.SizeOf<T>`, `PtrToStructure<T>` and `GetDelegateForFunctionPointer<T>` overloads are available.
    pub generic_marshal: bool,
    /// `TaskCreationOptions.RunContinuationsAsynchronously` and `TaskCompletionSource.TrySetCanceled(CancellationToken)` are available.
    pub run_continuations_asynchronously: bool,
    /// `IAsyncEnumerable<T>` is available.
    pub async_streams: bool,
    /// `Array.Empty<T>()` is available.
    pub array_empty: bool,
}

impl CSharpFeatures {
    /// Returns whether generated code has to ship stand-ins for runtime types.
    pub const fn needs_polyfills(self) -> bool {
        !self.is_external_init || !self.overload_resolution_priority
    }

    /// Spells an empty array of the given element type.
    pub fn empty_array(self, element: &str) -> String {
        match self.array_empty {
            true => format!("global::System.Array.Empty<{element}>()"),
            false => format!("BoltFFIEmptyArray<{element}>.Value"),
        }
    }

    /// Spells the unmanaged byte size of a blittable element type.
    pub fn element_size(self, element: &str) -> String {
        match self.system_memory {
            true => format!("global::System.Runtime.CompilerServices.Unsafe.SizeOf<{element}>()"),
            false => self.marshal_size_of(element),
        }
    }

    /// Spells the marshalled byte size of a struct type.
    pub fn marshal_size_of(self, ty: &str) -> String {
        match self.generic_marshal {
            true => format!("global::System.Runtime.InteropServices.Marshal.SizeOf<{ty}>()"),
            false => format!("global::System.Runtime.InteropServices.Marshal.SizeOf(typeof({ty}))"),
        }
    }

    /// Spells reading a struct from native memory.
    pub fn ptr_to_structure(self, ty: &str, pointer: &str) -> String {
        match self.generic_marshal {
            true => format!(
                "global::System.Runtime.InteropServices.Marshal.PtrToStructure<{ty}>({pointer})"
            ),
            false => format!(
                "({ty})global::System.Runtime.InteropServices.Marshal.PtrToStructure({pointer}, typeof({ty}))"
            ),
        }
    }

    /// Spells wrapping a native function pointer in a delegate.
    pub fn delegate_for_pointer(self, delegate: &str, pointer: &str) -> String {
        match self.generic_marshal {
            true => format!(
                "global::System.Runtime.InteropServices.Marshal.GetDelegateForFunctionPointer<{delegate}>({pointer})"
            ),
            false => format!(
                "({delegate})global::System.Runtime.InteropServices.Marshal.GetDelegateForFunctionPointer({pointer}, typeof({delegate}))"
            ),
        }
    }

    /// Spells the type of a completion source whose continuations never run inline.
    pub fn completion_source_type(self, result: &str) -> String {
        match self.run_continuations_asynchronously {
            true => format!("global::System.Threading.Tasks.TaskCompletionSource<{result}>"),
            false => format!("BoltFFICompletionSource<{result}>"),
        }
    }

    /// Spells constructing a completion source whose continuations never run inline.
    pub fn completion_source(self, result: &str) -> String {
        match self.run_continuations_asynchronously {
            true => format!(
                "new global::System.Threading.Tasks.TaskCompletionSource<{result}>(global::System.Threading.Tasks.TaskCreationOptions.RunContinuationsAsynchronously)"
            ),
            false => format!("new BoltFFICompletionSource<{result}>()"),
        }
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
        if NETFX45_FRAMEWORKS.contains(&moniker) {
            return Ok(Self::NetFx45);
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
                system_memory: true,
                generic_marshal: true,
                run_continuations_asynchronously: true,
                async_streams: true,
                array_empty: true,
            },
            Self::NetFx46 => CSharpFeatures {
                marshal_utf8_strings: false,
                bit_converter_single_bits: false,
                unix_epoch: false,
                guid_big_endian: false,
                interlocked_ulong: false,
                is_external_init: false,
                overload_resolution_priority: false,
                system_memory: true,
                generic_marshal: true,
                run_continuations_asynchronously: true,
                async_streams: true,
                array_empty: true,
            },
            Self::NetFx45 => CSharpFeatures {
                marshal_utf8_strings: false,
                bit_converter_single_bits: false,
                unix_epoch: false,
                guid_big_endian: false,
                interlocked_ulong: false,
                is_external_init: false,
                overload_resolution_priority: false,
                system_memory: false,
                generic_marshal: false,
                run_continuations_asynchronously: false,
                async_streams: false,
                array_empty: false,
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
    fn framework_4_5_selects_the_netfx45_profile() {
        for framework in ["net45", "net451", "net452", "net46", "NET45"] {
            assert_eq!(
                CSharpProfile::from_target_framework(framework),
                Ok(CSharpProfile::NetFx45),
                "{framework}"
            );
        }
    }

    #[test]
    fn older_and_unknown_frameworks_are_rejected() {
        for framework in [
            "",
            "net",
            "net453",
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
    fn netfx45_lacks_the_apis_added_after_4_5() {
        let features = CSharpProfile::NetFx45.features();
        assert!(!features.system_memory);
        assert!(!features.generic_marshal);
        assert!(!features.run_continuations_asynchronously);
        assert!(!features.async_streams);
        assert!(!features.array_empty);
        let netfx46 = CSharpProfile::NetFx46.features();
        assert!(netfx46.system_memory && netfx46.generic_marshal && netfx46.array_empty);
        assert!(netfx46.run_continuations_asynchronously && netfx46.async_streams);
    }

    #[test]
    fn features_spell_apis_that_exist_on_the_profile() {
        let modern = CSharpProfile::Modern.features();
        let legacy = CSharpProfile::NetFx45.features();
        assert_eq!(
            modern.empty_array("byte"),
            "global::System.Array.Empty<byte>()"
        );
        assert_eq!(legacy.empty_array("byte"), "BoltFFIEmptyArray<byte>.Value");
        assert!(modern.element_size("int").contains("Unsafe.SizeOf<int>()"));
        assert!(
            legacy
                .element_size("int")
                .ends_with("Marshal.SizeOf(typeof(int))")
        );
        assert!(modern.marshal_size_of("Point").contains("SizeOf<Point>()"));
        assert!(
            legacy
                .marshal_size_of("Point")
                .contains("SizeOf(typeof(Point))")
        );
        assert!(
            modern
                .delegate_for_pointer("Fn", "pointer")
                .contains("GetDelegateForFunctionPointer<Fn>(pointer)")
        );
        assert_eq!(
            legacy.delegate_for_pointer("Fn", "pointer"),
            "(Fn)global::System.Runtime.InteropServices.Marshal.GetDelegateForFunctionPointer(pointer, typeof(Fn))"
        );
        assert!(
            modern
                .completion_source("int")
                .contains("RunContinuationsAsynchronously")
        );
        assert_eq!(
            legacy.completion_source("int"),
            "new BoltFFICompletionSource<int>()"
        );
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
