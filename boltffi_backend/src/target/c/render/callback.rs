//! Ergonomic C wrappers for callback traits and closure parameters.
//!
//! The ABI exposes callback traits as a vtable typedef, a global registration
//! function, and a generic handle carrier. The host re-exports the vtable and
//! wraps that carrier in a package- and trait-specific struct, preventing an
//! unrelated callback trait from being passed to the wrong ergonomic function.

use boltffi_binding::{CallbackDecl, CallbackId, Native};

use crate::{
    bridge::c::{self, ParameterGroup},
    core::{Emitted, Error, RenderContext, Result},
    target::c::name_style::Name,
};

use super::prefix::PackagePrefix;

/// Returns the ergonomic typed-handle spelling for one callback trait.
pub fn handle_type_name(id: CallbackId, context: &RenderContext<Native>) -> Result<String> {
    let callback = context.callback(id).ok_or(Error::BrokenBridgeContract {
        bridge: "c",
        invariant: "callback handle target has no callback declaration",
    })?;
    let prefix = PackagePrefix::from_context(context);
    let friendly = prefix.type_name(&Name::new(callback.name()).r#type());
    Ok(format!("{friendly}Handle"))
}

/// Renders one callback trait's ergonomic surface.
pub fn render(
    decl: &CallbackDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let callback = bridge
        .source_callback(decl.id())
        .ok_or(Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "missing callback protocol",
        })?;

    // Sync-only guard: async callback methods emit completion payloads that are
    // out of scope. Reject the whole trait rather than partially rendering.
    let has_async = callback.methods().iter().any(|slot| {
        slot.parameter_groups()
            .iter()
            .chain(slot.return_parameter_groups())
            .any(|group| matches!(group, ParameterGroup::CallbackCompletion(_)))
    });
    if has_async {
        return Err(Error::UnsupportedTarget {
            target: "c",
            shape: "async callback methods are out of scope",
        });
    }

    let prefix = PackagePrefix::from_context(context);
    let vtable_name = callback.vtable().name();
    let friendly = prefix.type_name(&Name::new(callback.name()).r#type());
    let handle = handle_type_name(decl.id(), context)?;
    let member = Name::new(callback.name()).member();
    let create = crate::bridge::c::Identifier::parse(prefix.member(&format!("{member}_create")))?;
    let register_name = callback.register().name();
    let create_name = callback.create_handle().name();

    let mut chunk = format!(
        "typedef {vtable_name} {friendly};\ntypedef struct {{\n    BoltFFICallbackHandle raw;\n}} {handle};\n"
    );
    chunk.push_str(&format!(
        "/* A {friendly} vtable. The caller fills the function-pointer slots, then\n * passes it to {create} together with a non-zero identity that the exported\n * Rust side uses as the callback's handle. The returned {handle} must be kept\n * alive for the duration of the call that consumes it.\n */\n"
    ));
    chunk.push_str(&format!(
        "static inline {handle} {create}(const {friendly} *vtable, uint64_t identity) {{\n    {handle} result;\n    {register_name}((const {vtable_name} *)vtable);\n    result.raw = {create_name}(identity);\n    return result;\n}}\n"
    ));
    Ok(Emitted::primary(chunk))
}
