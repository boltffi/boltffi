use boltffi_ffi_rules::naming::{LibraryName, Name};

use super::super::ast::{CSharpClassName, CSharpNamespace};
use super::{CFunctionName, CSharpClassPlan, CSharpEnumPlan, CSharpFunctionPlan, CSharpRecordPlan};

/// A whole C# module: namespace, library binding, and every record, enum,
/// and function it exposes. Renders into a `namespace` spread across
/// multiple `.cs` files: one per record (`{record_name}.cs`), one per enum
/// (`{enum_name}.cs`), and a shared file (`{class_name}.cs`) holding the
/// function wrappers, the `NativeMethods` DllImport class, and the
/// runtime helpers (`FfiBuf`, `WireReader`, `WireWriter`) gated by the
/// `needs_*` predicates.
#[derive(Debug, Clone)]
pub struct CSharpModulePlan {
    /// Namespace for the generated files.
    pub namespace: CSharpNamespace,
    /// Top-level wrapper class name.
    pub class_name: CSharpClassName,
    /// Native library name used in `[DllImport("...")]` declarations.
    pub lib_name: Name<LibraryName>,
    /// C function that frees the buffer used by wire-encoded returns.
    pub free_buf_ffi_name: CFunctionName,
    /// Records exposed by the module. Each record is rendered to its own
    /// `.cs` file as a `readonly record struct`.
    pub records: Vec<CSharpRecordPlan>,
    /// Enums exposed by the module. Each enum is rendered to its own `.cs`
    /// file: C-style as a native `enum`, data-carrying as an
    /// `abstract record` with nested `sealed record` variants.
    pub enums: Vec<CSharpEnumPlan>,
    /// Top-level functions exposed by the module.
    pub functions: Vec<CSharpFunctionPlan>,
    /// Classes exposed by the module. Each class is rendered to its
    /// own `.cs` file as a `sealed class` implementing `IDisposable`
    /// around an opaque native handle.
    pub classes: Vec<CSharpClassPlan>,
}

impl CSharpModulePlan {
    /// Whether the module exposes any functions. Gates the wrapper-class
    /// file in the functions template.
    pub fn has_functions(&self) -> bool {
        !self.functions.is_empty()
    }

    /// Whether the module exposes any classes. Gates the per-class
    /// `[DllImport]` block in the native template.
    pub fn has_classes(&self) -> bool {
        !self.classes.is_empty()
    }

    /// Whether the module needs `using System.Text;`. True when any function
    /// or class member touches a string (param or wire-decoded return), or
    /// any record has a string field, since `Encoding.UTF8.GetBytes` lives
    /// there. Decoding does not need `System.Text`; `WireReader` reads
    /// strings via `Marshal.PtrToStringUTF8`.
    pub fn needs_system_text(&self) -> bool {
        self.functions
            .iter()
            .any(|f| f.params.iter().any(|p| p.csharp_type.contains_string()))
            || self.classes.iter().any(CSharpClassPlan::needs_system_text)
            || self.records.iter().any(CSharpRecordPlan::has_string_fields)
    }

    /// Whether any function, class constructor, or class method takes a
    /// wire-encoded param. Blittable record params pass through the CLR
    /// as direct struct values and do not contribute here.
    fn has_wire_params(&self) -> bool {
        self.functions.iter().any(|f| !f.wire_writers.is_empty())
            || self.classes.iter().any(CSharpClassPlan::has_wire_params)
    }

    /// Whether any function returns through an `FfiBuf`, a wire-decoded
    /// string or non-blittable record. Blittable records come back as
    /// direct struct values and do not count here.
    fn has_ffi_buf_returns(&self) -> bool {
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

    /// Whether the runtime `BoltException` class is emitted. True when
    /// any throwing function or method in the module ends up calling
    /// `new BoltException(...)` — i.e., any `Result<_, _>` whose Err
    /// type isn't a typed `#[error]` enum or record. Mirrors the
    /// Kotlin/Swift/Dart pattern of a generated runtime FFI exception
    /// type; Java reuses the built-in `RuntimeException` instead and
    /// has no equivalent.
    pub fn needs_bolt_exception(&self) -> bool {
        self.functions.iter().any(|f| f.return_kind.is_result())
            || self
                .classes
                .iter()
                .any(CSharpClassPlan::has_throwing_methods)
            || self
                .records
                .iter()
                .any(CSharpRecordPlan::has_throwing_methods)
            || self.enums.iter().any(CSharpEnumPlan::has_throwing_methods)
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::ast::{
        CSharpExpression, CSharpIdentity, CSharpLocalName, CSharpMethodName, CSharpType,
    };
    use super::super::{
        CFunctionName, CSharpEnumKind, CSharpFunctionPlan, CSharpMethodPlan, CSharpReceiver,
        CSharpReturnKind,
    };
    use super::*;

    fn dummy_throw_expr() -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("placeholder")))
    }

    fn empty_module() -> CSharpModulePlan {
        CSharpModulePlan {
            namespace: CSharpNamespace::from_source("Demo"),
            class_name: CSharpClassName::from_source("demo"),
            lib_name: Name::new("demo".to_string()),
            free_buf_ffi_name: CFunctionName::new("boltffi_free_buf".to_string()),
            records: vec![],
            enums: vec![],
            functions: vec![],
            classes: vec![],
        }
    }

    fn throwing_function() -> CSharpFunctionPlan {
        CSharpFunctionPlan {
            summary_doc: None,
            name: CSharpMethodName::from_source("test"),
            params: vec![],
            return_type: CSharpType::Int,
            return_kind: CSharpReturnKind::WireDecodeResult {
                ok_decode_expr: None,
                err_throw_expr: dummy_throw_expr(),
            },
            ffi_name: CFunctionName::new("boltffi_test".to_string()),
            wire_writers: vec![],
        }
    }

    fn throwing_method() -> CSharpMethodPlan {
        CSharpMethodPlan {
            summary_doc: None,
            name: CSharpMethodName::from_source("test"),
            native_method_name: CSharpMethodName::from_source("OwnerTest"),
            ffi_name: CFunctionName::new("boltffi_test".to_string()),
            receiver: CSharpReceiver::ClassInstance,
            params: vec![],
            return_type: CSharpType::Void,
            return_kind: CSharpReturnKind::WireDecodeResult {
                ok_decode_expr: None,
                err_throw_expr: dummy_throw_expr(),
            },
            wire_writers: vec![],
            owner_is_blittable: false,
        }
    }

    fn throwing_class_plan() -> CSharpClassPlan {
        CSharpClassPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("counter"),
            ffi_free: CFunctionName::new("boltffi_counter_free".to_string()),
            native_free_method_name: CSharpMethodName::from_source("CounterFree"),
            constructors: vec![],
            methods: vec![throwing_method()],
        }
    }

    fn throwing_record_plan() -> CSharpRecordPlan {
        CSharpRecordPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("dataset"),
            is_blittable: false,
            fields: vec![],
            methods: vec![throwing_method()],
            is_error: false,
        }
    }

    fn throwing_enum_plan() -> CSharpEnumPlan {
        CSharpEnumPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("status"),
            wire_class_name: CSharpClassName::from_source("status_wire"),
            methods_class_name: None,
            kind: CSharpEnumKind::CStyle,
            underlying_type: None,
            variants: vec![],
            methods: vec![throwing_method()],
            is_error: false,
        }
    }

    /// A module with no throwing functions or members doesn't need the
    /// runtime `BoltException` class. Pinning the negative case prevents
    /// the predicate from drifting into "always true" and unconditionally
    /// emitting the class.
    #[test]
    fn needs_bolt_exception_is_false_for_empty_module() {
        assert!(!empty_module().needs_bolt_exception());
    }

    /// A throwing top-level function flips the predicate. Function
    /// wrappers can throw `BoltException` directly even when no class /
    /// record / enum has a throwing method, so the function path has to
    /// trigger the runtime class on its own.
    #[test]
    fn needs_bolt_exception_is_true_when_a_function_returns_result() {
        let mut module = empty_module();
        module.functions.push(throwing_function());
        assert!(module.needs_bolt_exception());
    }

    /// A class with a throwing method flips the predicate. The
    /// generated wrapper reaches `throw new BoltException(...)` from
    /// inside the class even if no top-level function does.
    #[test]
    fn needs_bolt_exception_is_true_when_a_class_method_returns_result() {
        let mut module = empty_module();
        module.classes.push(throwing_class_plan());
        assert!(module.needs_bolt_exception());
    }

    /// A record method that returns `Result<_, _>` flips the predicate
    /// — record methods aren't on classes, so the class-only check
    /// would miss them and the runtime class would silently not emit.
    #[test]
    fn needs_bolt_exception_is_true_when_a_record_method_returns_result() {
        let mut module = empty_module();
        module.records.push(throwing_record_plan());
        assert!(module.needs_bolt_exception());
    }

    /// Same for enum methods. Pinning all four input sources separately
    /// catches the case where someone refactors the predicate and
    /// forgets one branch.
    #[test]
    fn needs_bolt_exception_is_true_when_an_enum_method_returns_result() {
        let mut module = empty_module();
        module.enums.push(throwing_enum_plan());
        assert!(module.needs_bolt_exception());
    }
}
