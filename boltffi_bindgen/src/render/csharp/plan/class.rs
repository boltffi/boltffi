use super::super::ast::{
    CSharpArgumentList, CSharpClassName, CSharpComment, CSharpMethodName, CSharpParameterList,
};
use super::callable::{native_call_arg_list, native_param_list};
use super::{
    CFunctionName, CSharpCallablePlan, CSharpMethodPlan, CSharpParamPlan, CSharpStreamPlan,
    CSharpWireWriterPlan,
};

/// A Rust object exposed as a C# `IDisposable` wrapper around an
/// opaque native handle (`IntPtr`), emitted to its own `.cs` file.
///
/// The wrapper owns the handle for the lifetime of the managed
/// instance and frees it through the C-side `_free` symbol when
/// `Dispose` is called (or, as a safety net, when the finalizer
/// runs because the consumer forgot to dispose).
///
/// Examples:
/// ```csharp
/// public sealed class Inventory : IDisposable
/// {
///     private IntPtr _handle;
///     internal Inventory(IntPtr handle) { _handle = handle; }
///     public Inventory() : this(NewHandle()) { }
///     public static Inventory WithCapacity(uint capacity) =>
///         new Inventory(NativeMethods.InventoryWithCapacity(capacity));
///     public void Dispose() { ... }
///     ~Inventory() { Dispose(); }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CSharpClassPlan {
    /// Renders a `<summary>` block comment, when `Some`.
    pub summary_doc: Option<CSharpComment>,
    /// Class name (e.g., `"Inventory"`).
    pub class_name: CSharpClassName,
    /// C-side symbol that frees the native handle.
    pub ffi_free: CFunctionName,
    /// `[DllImport]` entry name used inside `NativeMethods` for the
    /// free function. Two classes may declare the same free shape, so
    /// the owner class name is prefixed (`InventoryFree`,
    /// `CounterFree`).
    pub native_free_method_name: CSharpMethodName,
    /// Public constructors exposed on the wrapper. A `Default` Rust
    /// constructor lifts to a [`CSharpConstructorKind::Primary`] C#
    /// instance constructor; named factories and named-init
    /// constructors lift to [`CSharpConstructorKind::StaticFactory`]
    /// methods on the class.
    pub constructors: Vec<CSharpConstructorPlan>,
    /// Public methods exposed on the wrapper. Instance methods carry
    /// [`super::CSharpReceiver::ClassInstance`] so the rendered body
    /// passes `_handle` as the first native arg; static methods carry
    /// [`super::CSharpReceiver::Static`] and behave like free functions
    /// scoped to the class.
    pub methods: Vec<CSharpMethodPlan>,
    /// Public stream subscriptions exposed on the wrapper.
    pub streams: Vec<CSharpStreamPlan>,
}

/// One public way to construct an instance of the wrapper. Calls the
/// matching native `_new`-family symbol, then wraps the returned
/// `IntPtr` in `new ClassName(handle)`.
///
/// Examples:
/// ```csharp
/// // Primary: rendered as a real C# instance constructor that
/// // delegates to the internal `IntPtr` ctor through a private
/// // helper. The helper hosts any wire-encoding setup that the
/// // chained-ctor `: this(...)` form cannot fit in an expression.
/// public Inventory() : this(NewHandle()) { }
/// private static IntPtr NewHandle() => NativeMethods.InventoryNew();
///
/// // StaticFactory: rendered as a `public static` method that
/// // returns a fresh wrapper.
/// public static Inventory WithCapacity(uint capacity) =>
///     new Inventory(NativeMethods.InventoryWithCapacity(capacity));
/// ```
#[derive(Debug, Clone)]
pub struct CSharpConstructorPlan {
    /// Renders a `<summary>` block comment, when `Some`.
    pub summary_doc: Option<CSharpComment>,
    /// How the constructor is rendered on the public surface.
    pub kind: CSharpConstructorKind,
    /// Name used for this constructor's DllImport entry inside the
    /// shared `NativeMethods` class. Prefixed with the owning class
    /// name (`InventoryNew`, `InventoryWithCapacity`) because the
    /// DllImport class is flat.
    pub native_method_name: CSharpMethodName,
    /// The C function this constructor calls across the ABI.
    pub ffi_name: CFunctionName,
    /// Explicit params on the public surface.
    pub params: Vec<CSharpParamPlan>,
    /// For each non-blittable record / data-enum / option / nested-vec
    /// param, the setup block that wire-encodes it into a `byte[]`
    /// before the native call.
    pub wire_writers: Vec<CSharpWireWriterPlan>,
}

/// How a constructor renders on the public surface of the wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpConstructorKind {
    /// A real C# instance constructor: `public ClassName(...)`. Lifts
    /// from a Rust `Default` constructor (the conventional `pub fn
    /// new(...) -> Self`). Delegates to the internal `IntPtr` ctor
    /// through a private static helper so any wire-encoding setup has
    /// somewhere to live; the chained-ctor `: this(...)` syntax only
    /// accepts a single expression. `helper_method_name` is the name
    /// of that helper (e.g., `"InventoryNewHandle"`).
    Primary {
        helper_method_name: CSharpMethodName,
    },
    /// A static factory: `public static ClassName Name(...)`. Lifts
    /// from named factories (`pub fn empty() -> Self`) and named-init
    /// constructors (`pub fn with_capacity(u32) -> Self`). The factory
    /// body inlines any setup before returning a fresh wrapper, so no
    /// helper is needed.
    StaticFactory { name: CSharpMethodName },
}

impl CSharpConstructorPlan {
    /// Typed param list for the `[DllImport]` native signature.
    pub fn native_param_list(&self) -> CSharpParameterList {
        native_param_list(&self.params)
    }

    /// Argument list passed to the native call. Mirrors the shape
    /// produced by [`Self::native_param_list`]: a `string` param
    /// expands into `(name_bytes, (UIntPtr)name_bytes.Length)`, a
    /// wire-encoded record into `(_nameBytes, (UIntPtr)_nameBytes.Length)`,
    /// and so on.
    pub fn native_call_args(&self) -> CSharpArgumentList {
        native_call_arg_list(&self.params)
    }

    /// Whether any param requires an `unsafe { fixed (...) { ... } }`
    /// block around the native call. Drives the same scaffolding the
    /// top-level functions template uses for `Vec<BlittableRecord>`
    /// params.
    pub fn has_pinned_params(&self) -> bool {
        self.params.iter().any(CSharpParamPlan::is_pinned)
    }

    /// Whether any param contributes a usage of `Encoding.UTF8` to
    /// the constructor body. Drives the conditional `using System.Text;`
    /// directive emitted by the class template. True when a param's
    /// type contains a `string` at any nesting depth: a plain `string`
    /// triggers `Encoding.UTF8.GetBytes` setup, and a `Vec<string>` or
    /// option-wrapped string contributes `Encoding.UTF8.GetByteCount`
    /// inside its wire-writer size expression.
    pub fn needs_system_text(&self) -> bool {
        self.params.iter().any(|p| p.csharp_type.contains_string())
    }
}

impl CSharpClassPlan {
    /// Whether any constructor or method has a pinned-array param.
    /// Drives the conditional `using System.Runtime.CompilerServices`
    /// import (for `Unsafe.SizeOf<T>`) and the `unsafe { fixed }`
    /// scaffolding in the class template.
    pub fn has_pinned_params(&self) -> bool {
        self.constructors
            .iter()
            .any(CSharpConstructorPlan::has_pinned_params)
            || self.methods.iter().any(CSharpMethodPlan::has_pinned_params)
    }

    /// Whether any constructor or method needs `using System.Text;` in
    /// the class file. True when any param's type contains a `string`
    /// (drives `Encoding.UTF8.GetBytes` or `Encoding.UTF8.GetByteCount`
    /// somewhere in the body) or when any method's return value
    /// wire-decodes a string.
    pub fn needs_system_text(&self) -> bool {
        self.constructors
            .iter()
            .any(CSharpConstructorPlan::needs_system_text)
            || self.methods.iter().any(method_needs_system_text)
    }

    /// Whether any constructor or method takes a wire-encoded param
    /// (and so needs the `WireWriter` helper at module scope). Mirrors
    /// `CSharpFunctionPlan::has_wire_params` but spans both members.
    pub fn has_wire_params(&self) -> bool {
        self.constructors.iter().any(|c| !c.wire_writers.is_empty())
            || self.methods.iter().any(|m| !m.wire_writers.is_empty())
    }

    /// Whether any method returns `Result<_, _>`. Used by the
    /// module-level predicate that decides whether to emit the runtime
    /// `BoltException` class.
    pub fn has_throwing_methods(&self) -> bool {
        self.methods.iter().any(|m| m.return_kind.is_result())
    }

    pub fn has_async_methods(&self) -> bool {
        self.methods.iter().any(CSharpMethodPlan::is_async)
    }

    pub fn has_streams(&self) -> bool {
        !self.streams.is_empty()
    }
}

/// Whether a class method needs `using System.Text;`: true if any
/// param contains a `string` (the wire-encode size path uses
/// `Encoding.UTF8.GetByteCount`) or the return type contains a
/// `string`. The return-side check covers methods that decode through
/// `WireReader` since their bodies don't reference `Encoding` directly,
/// but record-typed returns may; covering "any param or return touches
/// string" is the conservative match for the predicate.
fn method_needs_system_text(method: &CSharpMethodPlan) -> bool {
    method
        .params
        .iter()
        .any(|p| p.csharp_type.contains_string())
        || method.return_type.contains_string()
}

#[cfg(test)]
mod tests {
    use super::super::super::ast::{CSharpExpression, CSharpIdentity, CSharpLocalName, CSharpType};
    use super::super::CSharpReturnKind;
    use super::*;

    fn dummy_throw_expr() -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("placeholder")))
    }

    fn class_with_methods(methods: Vec<CSharpMethodPlan>) -> CSharpClassPlan {
        CSharpClassPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("counter"),
            ffi_free: CFunctionName::new("boltffi_counter_free".to_string()),
            native_free_method_name: CSharpMethodName::from_source("CounterFree"),
            constructors: vec![],
            methods,
            streams: vec![],
        }
    }

    fn method_with_return_kind(return_kind: CSharpReturnKind) -> CSharpMethodPlan {
        CSharpMethodPlan {
            summary_doc: None,
            name: CSharpMethodName::from_source("test"),
            native_method_name: CSharpMethodName::from_source("CounterTest"),
            ffi_name: CFunctionName::new("boltffi_test".to_string()),
            async_call: None,
            receiver: super::super::CSharpReceiver::ClassInstance,
            params: vec![],
            return_type: CSharpType::Void,
            return_kind,
            wire_writers: vec![],
            owner_is_blittable: false,
        }
    }

    /// A class method whose return_kind is `WireDecodeResult` flips
    /// `has_throwing_methods` so the module predicate emits the runtime
    /// `BoltException` class. This is the path the demo crate's
    /// `Counter::try_get_positive` exercises.
    #[test]
    fn has_throwing_methods_is_true_when_a_class_method_is_a_result() {
        let class = class_with_methods(vec![method_with_return_kind(
            CSharpReturnKind::WireDecodeResult {
                ok_decode_expr: None,
                err_throw_expr: dummy_throw_expr(),
            },
        )]);
        assert!(class.has_throwing_methods());
    }

    /// Non-result return kinds don't flip the predicate. Pins that the
    /// throwing-methods check keys on the throwing shape specifically,
    /// not on wire decoding generally.
    #[test]
    fn has_throwing_methods_is_false_when_no_class_method_is_a_result() {
        let class = class_with_methods(vec![
            method_with_return_kind(CSharpReturnKind::Direct),
            method_with_return_kind(CSharpReturnKind::WireDecodeString),
        ]);
        assert!(!class.has_throwing_methods());
    }
}
