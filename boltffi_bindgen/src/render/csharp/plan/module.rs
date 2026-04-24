//! [`CSharpModulePlan`]: the top-level aggregator. Holds the records,
//! enums, and functions the templates render, plus the handful of
//! boolean flags that decide which shared runtime helpers (`FfiBuf`,
//! `WireReader`, `WireWriter`, `System.Text`) get emitted.

use boltffi_ffi_rules::naming::{LibraryName, Name};

use super::super::ast::{CSharpClassName, CSharpNamespace};
use super::{CFunctionName, CSharpEnumPlan, CSharpFunctionPlan, CSharpRecordPlan};

/// Represents a lowered C# module, containing everything the templates need
/// to render a `.cs` file.
#[derive(Debug, Clone)]
pub struct CSharpModulePlan {
    /// Namespace for the generated files.
    pub namespace: CSharpNamespace,
    /// Top-level wrapper class name.
    pub class_name: CSharpClassName,
    /// Native library name used in `[DllImport("...")]` declarations.
    pub lib_name: Name<LibraryName>,
    /// C function that frees the buffer used by wire-encoded returns.
    /// Pre-computed so the native template doesn't concatenate the
    /// shared FFI prefix with a literal suffix at render time.
    pub free_buf_ffi_name: CFunctionName,
    /// Records exposed by the module. Each record is rendered to its own
    /// `.cs` file as a `readonly record struct`.
    pub records: Vec<CSharpRecordPlan>,
    /// Enums exposed by the module. Each enum is rendered to its own `.cs`
    /// file: C-style as a native `enum`, data-carrying as an
    /// `abstract record` with nested `sealed record` variants.
    pub enums: Vec<CSharpEnumPlan>,
    /// Top-level primitive functions. Used by both the public wrapper class
    /// and the `[DllImport]` native declarations: C# P/Invoke passes
    /// primitives directly, so one struct serves both layers.
    pub functions: Vec<CSharpFunctionPlan>,
}

impl CSharpModulePlan {
    pub fn has_functions(&self) -> bool {
        !self.functions.is_empty()
    }

    /// Whether the shared runtime helpers need `System.Text`.
    ///
    /// Top-level string params use `Encoding.UTF8.GetBytes` in the wrapper,
    /// and `WireWriter` uses `Encoding.UTF8.GetByteCount` / `GetBytes` when
    /// encoding string-bearing params (including `Vec<String>` / nested
    /// string vecs) or string fields of a record. Decoding no longer needs
    /// `System.Text`. `WireReader` reads strings through
    /// `Marshal.PtrToStringUTF8`.
    pub fn needs_system_text(&self) -> bool {
        self.functions
            .iter()
            .any(|f| f.params.iter().any(|p| p.csharp_type.contains_string()))
            || self.records.iter().any(CSharpRecordPlan::has_string_fields)
    }

    /// Whether any function takes a wire-encoded record param. Blittable
    /// record params pass through the CLR as direct struct values and do
    /// not contribute here.
    pub fn has_wire_params(&self) -> bool {
        self.functions.iter().any(|f| !f.wire_writers.is_empty())
    }

    /// Whether any function returns through an `FfiBuf`, a wire-decoded
    /// string or non-blittable record. Blittable records come back as
    /// direct struct values and do not count here.
    pub fn has_ffi_buf_returns(&self) -> bool {
        self.functions
            .iter()
            .any(|f| f.return_kind.native_returns_ffi_buf())
    }

    /// Whether the `FfiBuf` struct and `FreeBuf` DllImport are emitted.
    /// Needed for wire-encoded returns, and pulled in whenever a record or
    /// enum exists so the `WireReader` (which takes `FfiBuf`) compiles.
    pub fn needs_ffi_buf(&self) -> bool {
        self.has_ffi_buf_returns() || !self.records.is_empty() || !self.enums.is_empty()
    }

    /// Whether the stateful `WireReader` helper is emitted. Needed for
    /// wire-decoded returns, for any record's `Decode` method, and for the
    /// enum wire helpers (`StatusWire.Decode`, `Shape.Decode`).
    pub fn needs_wire_reader(&self) -> bool {
        self.has_ffi_buf_returns() || !self.records.is_empty() || !self.enums.is_empty()
    }

    /// Whether the `WireWriter` helper is emitted. Needed for wire-encoded
    /// params, for any record's `WireEncodeTo` method, and for the enum
    /// encode helpers.
    pub fn needs_wire_writer(&self) -> bool {
        self.has_wire_params() || !self.records.is_empty() || !self.enums.is_empty()
    }
}
