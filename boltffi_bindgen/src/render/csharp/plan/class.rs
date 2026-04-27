use super::super::ast::{CSharpClassName, CSharpMethodName};
use super::CFunctionName;

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
///     public void Dispose() { ... }
///     ~Inventory() { Dispose(); }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CSharpClassPlan {
    /// Class name (e.g., `"Inventory"`).
    pub class_name: CSharpClassName,
    /// C-side symbol that frees the native handle.
    pub ffi_free: CFunctionName,
    /// `[DllImport]` entry name used inside `NativeMethods` for the
    /// free function. Two classes may declare the same free shape, so
    /// the owner class name is prefixed (`InventoryFree`,
    /// `CounterFree`).
    pub native_free_method_name: CSharpMethodName,
}
