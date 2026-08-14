//! Generates the Rust dispatch shim that makes Dart callback trait methods
//! safe to invoke from any OS thread, not just the isolate's own.
//!
//! See `boltffi_dart_runtime`'s crate-level docs for the full design. In
//! short: Dart's generated code registers a small hooks struct (the fast-
//! path and listener function pointers for one callback registration) via
//! `boltffi_dart_runtime_register_hooks`, keyed by the same `handle` value
//! the existing callback vtable dispatch already carries. This module emits,
//! per callback trait, one shim function per synchronous method whose ABI
//! shape (return + every parameter after `handle`) is scalar/void -- see
//! [`ShimMethod::from_slot`]. Async methods and non-scalar shapes are left
//! on the existing, unmodified `isolateLocal` path. `free`/`clone` are
//! vtable intrinsics, not user-declared methods, and are shimmed
//! unconditionally via [`ShimMethod::synthetic`] -- a `Send + Sync`
//! callback's last reference can be dropped on any thread.
//!
//! This is plain Rust, `include!()`'d into the `boltffi` facade crate by
//! its `build.rs` (behind the `dart` feature) -- not C compiled by a
//! separate toolchain.

use crate::bridge::c::{CBridgeContract, Callback, CallbackSlot, Parameter, Type};
use crate::core::Result;

const FREE_METHOD_NAME: &str = "free";
const CLONE_METHOD_NAME: &str = "clone";

const PRELUDE: &str = "\
// GENERATED FILE. Do not edit.
//
// Thread-safe dispatch shims for BoltFFI callback trait methods -- see
// `boltffi_dart_runtime`'s crate-level docs for the design. `include!()`'d
// into the `boltffi` crate by its build.rs; never invoked directly by
// application code.
";

/// Renders the whole module's shim source, or `None` if the bridge has no
/// callbacks at all.
pub(crate) fn render_module_shim(bridge: &CBridgeContract) -> Result<Option<String>> {
    let mut sections = Vec::new();
    for callback in bridge.callbacks() {
        let rendered = render_callback_shim(callback)?;
        if !rendered.is_empty() {
            sections.push(rendered);
        }
    }
    if sections.is_empty() {
        return Ok(None);
    }
    let mut out = PRELUDE.to_owned();
    out.push('\n');
    out.push_str(&sections.join("\n"));
    Ok(Some(out))
}

/// One C-ABI scalar type this module knows how to shim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarType {
    Void,
    Bool,
    Int8,
    Uint8,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Int64,
    Uint64,
    Float32,
    Float64,
}

impl ScalarType {
    fn from_c(ty: &Type) -> Option<Self> {
        Some(match ty {
            Type::Void => Self::Void,
            Type::Bool => Self::Bool,
            Type::Int8 => Self::Int8,
            Type::Uint8 => Self::Uint8,
            Type::Int16 => Self::Int16,
            Type::Uint16 => Self::Uint16,
            Type::Int32 => Self::Int32,
            Type::Uint32 => Self::Uint32,
            Type::Int64 => Self::Int64,
            Type::Uint64 => Self::Uint64,
            Type::Float32 => Self::Float32,
            Type::Float64 => Self::Float64,
            _ => return None,
        })
    }

    fn rust_name(self) -> &'static str {
        match self {
            Self::Void => "()",
            Self::Bool => "bool",
            Self::Int8 => "i8",
            Self::Uint8 => "u8",
            Self::Int16 => "i16",
            Self::Uint16 => "u16",
            Self::Int32 => "i32",
            Self::Uint32 => "u32",
            Self::Int64 => "i64",
            Self::Uint64 => "u64",
            Self::Float32 => "f32",
            Self::Float64 => "f64",
        }
    }

    fn zero_literal(self) -> &'static str {
        match self {
            Self::Void => "",
            Self::Bool => "false",
            Self::Float32 | Self::Float64 => "0.0",
            _ => "0",
        }
    }
}

/// One shimmable method. Owns its name rather than borrowing a
/// [`CallbackSlot`] so [`Self::synthetic`] can build one for `free`/`clone`,
/// which have no backing slot.
pub(crate) struct ShimMethod {
    name: String,
    parameters: Vec<(String, ScalarType)>,
    returns: ScalarType,
}

impl ShimMethod {
    /// Returns `None` for async methods or any method whose ABI shape isn't
    /// yet covered. Public within the target so
    /// `render::callback::method` can ask the same question -- both sides
    /// must agree on which methods get shim treatment.
    pub(crate) fn from_slot(slot: &CallbackSlot) -> Option<Self> {
        if slot.is_asynchronous() {
            return None;
        }
        // `parameters()[0]` is always `handle: uint64_t`.
        let rest = slot.parameters().get(1..)?;
        let parameters = rest
            .iter()
            .map(|parameter: &Parameter| {
                ScalarType::from_c(parameter.ty()).map(|ty| (parameter.name().to_owned(), ty))
            })
            .collect::<Option<Vec<_>>>()?;
        let returns = ScalarType::from_c(slot.returns())?;
        Some(Self {
            name: slot.name().to_string(),
            parameters,
            returns,
        })
    }

    /// Builds a shim method with no backing `CallbackSlot`, for `free`
    /// (`fn(u64)`) and `clone` (`fn(u64) -> u64`).
    pub(crate) fn synthetic(name: &str, returns: ScalarType) -> Self {
        Self {
            name: name.to_owned(),
            parameters: Vec::new(),
            returns,
        }
    }

    pub(crate) fn name(&self) -> String {
        self.name.clone()
    }

    pub(crate) fn is_void_return(&self) -> bool {
        matches!(self.returns, ScalarType::Void)
    }

    /// The fast-path function pointer type -- same shape as the existing
    /// `isolateLocal` target: `unsafe extern "C" fn(u64, ...params) -> Ret`.
    fn fast_fn_type(&self) -> String {
        let params = std::iter::once("u64".to_owned())
            .chain(
                self.parameters
                    .iter()
                    .map(|(_, ty)| ty.rust_name().to_owned()),
            )
            .collect::<Vec<_>>()
            .join(", ");
        if matches!(self.returns, ScalarType::Void) {
            format!("unsafe extern \"C\" fn({params})")
        } else {
            format!(
                "unsafe extern \"C\" fn({params}) -> {}",
                self.returns.rust_name()
            )
        }
    }

    /// The listener function pointer type: same leading params, plus a
    /// trailing gate pointer, plus (if non-void) a trailing out-pointer.
    fn listener_fn_type(&self) -> String {
        let mut params = vec!["u64".to_owned()];
        params.extend(
            self.parameters
                .iter()
                .map(|(_, ty)| ty.rust_name().to_owned()),
        );
        params.push("*mut ::std::ffi::c_void".to_owned());
        if !matches!(self.returns, ScalarType::Void) {
            params.push(format!("*mut {}", self.returns.rust_name()));
        }
        format!("unsafe extern \"C\" fn({})", params.join(", "))
    }
}

/// Rust source for one callback trait's dispatch shim: the hooks struct, its
/// registration/release functions, and one shim per method.
pub(crate) fn render_callback_shim(callback: &Callback) -> Result<String> {
    let methods: Vec<ShimMethod> =
        std::iter::once(ShimMethod::synthetic(FREE_METHOD_NAME, ScalarType::Void))
            .chain(std::iter::once(ShimMethod::synthetic(
                CLONE_METHOD_NAME,
                ScalarType::Uint64,
            )))
            .chain(
                callback
                    .methods()
                    .iter()
                    // `free`/`clone` are reserved for the synthetic entries
                    // above; guard against a collision outright.
                    .filter(|slot| {
                        slot.name().as_str() != FREE_METHOD_NAME
                            && slot.name().as_str() != CLONE_METHOD_NAME
                    })
                    .filter_map(ShimMethod::from_slot),
            )
            .collect();

    let type_name = shim_type_name(callback.vtable().name());
    let mut out = String::new();

    out.push_str(&format!(
        "// Dispatch shim for callback `{}`.\n",
        callback.vtable().name()
    ));
    out.push_str(&format!("#[repr(C)]\nstruct {type_name}Hooks {{\n"));
    out.push_str("    instance_handle: usize,\n");
    for method in &methods {
        let name = method.name();
        out.push_str(&format!("    {name}_fast: {},\n", method.fast_fn_type()));
        out.push_str(&format!(
            "    {name}_listener: {},\n",
            method.listener_fn_type()
        ));
    }
    out.push_str("}\n\n");

    let register_params = std::iter::once("handle: u64".to_owned())
        .chain(std::iter::once("instance_handle: usize".to_owned()))
        .chain(methods.iter().flat_map(|method| {
            let name = method.name();
            [
                format!("{name}_fast: {}", method.fast_fn_type()),
                format!("{name}_listener: {}", method.listener_fn_type()),
            ]
        }))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!(
        "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {type_name}_register({register_params}) {{\n"
    ));
    out.push_str(&format!(
        "    let hooks = ::std::boxed::Box::new({type_name}Hooks {{\n"
    ));
    out.push_str("        instance_handle,\n");
    for method in &methods {
        let name = method.name();
        out.push_str(&format!("        {name}_fast,\n"));
        out.push_str(&format!("        {name}_listener,\n"));
    }
    out.push_str("    });\n");
    out.push_str("    unsafe extern \"C\" fn free_hooks(ptr: *mut ::std::ffi::c_void) {\n");
    out.push_str(&format!(
        "        drop(unsafe {{ ::std::boxed::Box::from_raw(ptr as *mut {type_name}Hooks) }});\n"
    ));
    out.push_str("    }\n");
    out.push_str("    unsafe {\n");
    out.push_str("        ::boltffi_dart_runtime::boltffi_dart_runtime_register_hooks(\n");
    out.push_str("            handle as usize,\n");
    out.push_str("            instance_handle,\n");
    out.push_str("            ::std::boxed::Box::into_raw(hooks) as *mut ::std::ffi::c_void,\n");
    out.push_str("            free_hooks,\n");
    out.push_str("        );\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    out.push_str(&format!(
        "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {type_name}_release(handle: u64) {{\n"
    ));
    out.push_str("    if let Some(hooks_entry) = ::boltffi_dart_runtime::boltffi_dart_runtime_get_hooks_ref(handle as usize) {\n");
    out.push_str(&format!(
        "        let instance_handle = unsafe {{ &*(hooks_entry.ptr() as *const {type_name}Hooks) }}.instance_handle;\n"
    ));
    out.push_str(
        "        ::boltffi_dart_runtime::boltffi_dart_runtime_destroy_instance(instance_handle);\n",
    );
    out.push_str(
        "        ::boltffi_dart_runtime::boltffi_dart_runtime_forget_instance(instance_handle);\n",
    );
    out.push_str("    }\n");
    out.push_str(
        "    ::boltffi_dart_runtime::boltffi_dart_runtime_release_hooks(handle as usize);\n",
    );
    out.push_str("}\n\n");

    for method in &methods {
        out.push_str(&render_method_shim(&type_name, method));
        out.push('\n');
    }

    Ok(out)
}

/// The generated Rust type name prefix for one callback's shim -- also used
/// as the prefix for its `_register`/`_release` function symbols.
pub(crate) fn shim_type_name(name: &str) -> String {
    format!("BoltFFIDartShim_{name}")
}

/// The generated symbol name for one method's dispatch shim.
pub(crate) fn shim_symbol_name(callback_type_name: &str, method: &ShimMethod) -> String {
    format!("{callback_type_name}_{}", method.name())
}

/// The generated symbol name for one callback's hooks registration function.
pub(crate) fn register_symbol_name(vtable_name: &str) -> String {
    format!("{}_register", shim_type_name(vtable_name))
}

/// The generated symbol name for one callback's hooks release function.
pub(crate) fn release_symbol_name(vtable_name: &str) -> String {
    format!("{}_release", shim_type_name(vtable_name))
}

fn render_method_shim(type_name: &str, method: &ShimMethod) -> String {
    let name = method.name();
    let is_void = matches!(method.returns, ScalarType::Void);
    let ret = method.returns.rust_name();
    let zero = method.returns.zero_literal();

    let param_decls = std::iter::once("handle: u64".to_owned())
        .chain(
            method
                .parameters
                .iter()
                .map(|(name, ty)| format!("{name}: {}", ty.rust_name())),
        )
        .collect::<Vec<_>>()
        .join(", ");
    let arg_names = std::iter::once("handle".to_owned())
        .chain(method.parameters.iter().map(|(name, _)| name.clone()))
        .collect::<Vec<_>>()
        .join(", ");

    let signature = if is_void {
        format!("pub unsafe extern \"C\" fn {type_name}_{name}({param_decls})")
    } else {
        format!("pub unsafe extern \"C\" fn {type_name}_{name}({param_decls}) -> {ret}")
    };
    let early_return = if is_void {
        "return;".to_owned()
    } else {
        format!("return {zero};")
    };

    let mut body = String::new();
    body.push_str(&format!("#[unsafe(no_mangle)]\n{signature} {{\n"));
    // `hooks_entry` is kept as a normal local for the rest of this function,
    // including across the blocking `wait_gate` call -- Rust drops owned
    // locals at the function's lexical end, never sooner, which is what
    // stops a concurrent `_release` from freeing the hooks mid-call.
    body.push_str(&format!(
        "    let Some(hooks_entry) = ::boltffi_dart_runtime::boltffi_dart_runtime_get_hooks_ref(handle as usize) else {{ {early_return} }};\n"
    ));
    body.push_str(&format!(
        "    let hooks = unsafe {{ &*(hooks_entry.ptr() as *const {type_name}Hooks) }};\n"
    ));
    body.push_str("    if hooks_entry.instance().is_owner_thread() {\n");
    if is_void {
        body.push_str(&format!(
            "        unsafe {{ (hooks.{name}_fast)({arg_names}) }};\n        return;\n"
        ));
    } else {
        body.push_str(&format!(
            "        return unsafe {{ (hooks.{name}_fast)({arg_names}) }};\n"
        ));
    }
    body.push_str("    }\n");
    body.push_str("    let gate = hooks_entry.instance().create_gate().map(|gate| gate.raw()).unwrap_or(::std::ptr::null_mut());\n");
    body.push_str(&format!("    if gate.is_null() {{ {early_return} }}\n"));
    if is_void {
        body.push_str(&format!(
            "    unsafe {{ (hooks.{name}_listener)({arg_names}, gate) }};\n"
        ));
        body.push_str(
            "    unsafe { ::boltffi_dart_runtime::boltffi_dart_runtime_wait_gate(gate) };\n",
        );
        body.push_str("    return;\n");
    } else {
        body.push_str(&format!("    let mut out: {ret} = {zero};\n"));
        body.push_str(&format!(
            "    unsafe {{ (hooks.{name}_listener)({arg_names}, gate, &mut out as *mut {ret}) }};\n"
        ));
        body.push_str("    let status = unsafe { ::boltffi_dart_runtime::boltffi_dart_runtime_wait_gate(gate) };\n");
        body.push_str(&format!("    if status != 0 {{ return {zero}; }}\n"));
        body.push_str("    out\n");
    }
    body.push_str("}\n");
    body
}
