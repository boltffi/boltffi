pub mod method;
pub mod parameter;

use askama::Template;
use boltffi_binding::{CallbackDecl, Native};

use crate::{
    bridge::c::{CBridgeContract, Callback as CCallback},
    core::{Emitted, Error, RenderContext, Result},
};

use super::super::native;
use super::super::syntax::{Identifier, TypeFragment};
use super::{Documentation, declaration_name, indent};

use method::CallbackMethod;

#[derive(Template)]
#[template(path = "target/dart/callback.dart", escape = "none")]
struct CallbackTemplate<'a> {
    callback: &'a Callback,
}

pub struct Callback {
    documentation: Documentation,
    name: Identifier,
    bridge_name: Identifier,
    proxy_name: Identifier,
    register_declaration: String,
    create_declaration: String,
    register_name: Identifier,
    native_vtable: NativeVTable,
    interface_methods: Vec<String>,
    proxy_methods: Vec<String>,
    entries: Vec<String>,
    callables: Vec<String>,
    vtable_initializers: Vec<String>,
    free_vtable_initializer: String,
    clone_vtable_initializer: String,
    shim_declarations: Vec<String>,
    shim_register_call: String,
    shim_release_symbol: String,
}

struct NativeVTable {
    name: Identifier,
    fields: Vec<NativeVTableField>,
}

struct NativeVTableField {
    ty: TypeFragment,
    name: Identifier,
}

impl Callback {
    pub fn from_declaration(
        declaration: &CallbackDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let protocol =
            bridge
                .source_callback(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "Dart callback protocol is missing from the C bridge",
                })?;
        let source_methods = declaration.protocol().vtable().methods();
        if source_methods.len() != protocol.methods().len() {
            return broken("Dart callback method count disagrees with the C bridge");
        }
        // `free`/`clone` are the callback handle's own vtable intrinsics and
        // are also reserved by `render::shim` for its synthetic dispatch
        // entries; a trait method with either name would collide.
        if protocol
            .methods()
            .iter()
            .any(|slot| slot.name().as_str() == "free" || slot.name().as_str() == "clone")
        {
            return broken(
                "Dart callback trait has a method literally named `free` or `clone`, which \
                 collides with the callback handle's own built-in vtable slots of the same name",
            );
        }

        let name = declaration_name(declaration.name())?;
        let vtable_name = protocol.vtable().name();
        let free_clone = free_clone_wiring(vtable_name)?;
        let methods = source_methods
            .iter()
            .zip(protocol.methods())
            .map(|(method, slot)| {
                CallbackMethod::new(method, slot, &name, vtable_name, bridge, context)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc(), 0),
            bridge_name: Identifier::parse(format!("{name}Bridge"))?,
            proxy_name: Identifier::parse(format!("_{name}Proxy"))?,
            register_declaration: native::declaration(protocol.register())?,
            create_declaration: native::declaration(protocol.create_handle())?,
            register_name: Identifier::parse(protocol.register().name())?,
            native_vtable: NativeVTable::from_protocol(protocol)?,
            interface_methods: methods
                .iter()
                .map(|method| indent(method.interface(), 2))
                .collect(),
            proxy_methods: methods
                .iter()
                .map(|method| indent(method.proxy(), 2))
                .collect(),
            entries: methods
                .iter()
                .map(|method| indent(method.entry(), 2))
                .collect(),
            callables: methods
                .iter()
                .filter_map(CallbackMethod::callable)
                .map(|callable| indent(callable, 2))
                .collect(),
            vtable_initializers: methods
                .iter()
                .map(CallbackMethod::vtable_initializer)
                .map(|initializer| initializer.map(|initializer| indent(&initializer, 6)))
                .collect::<Result<Vec<_>>>()?,
            free_vtable_initializer: indent(&free_clone.free_vtable_initializer, 6),
            clone_vtable_initializer: indent(&free_clone.clone_vtable_initializer, 6),
            shim_declarations: {
                // `free`/`clone` first, then every qualifying user method --
                // must match `render::shim`'s hooks struct order.
                let mut declarations = vec![free_clone.declarations.clone()];
                declarations.extend(methods.iter().filter_map(CallbackMethod::shim_declarations));
                declarations.push(shim_register_release_declaration(
                    vtable_name,
                    &free_clone,
                    &methods,
                )?);
                declarations
                    .into_iter()
                    .map(|declaration| indent(&declaration, 2))
                    .collect()
            },
            shim_register_call: {
                let arguments = std::iter::once(free_clone.register_arguments.clone())
                    .chain(
                        methods
                            .iter()
                            .filter_map(CallbackMethod::shim_register_arguments)
                            .map(str::to_owned),
                    )
                    .collect::<Vec<_>>();
                format!(
                    "_f${}(handle, instanceHandle, {});",
                    super::shim::register_symbol_name(vtable_name),
                    arguments.join(", ")
                )
            },
            shim_release_symbol: super::shim::release_symbol_name(vtable_name),
            name,
        })
    }

    pub fn render(self) -> Emitted {
        Emitted::primary(
            CallbackTemplate { callback: &self }
                .render()
                .expect("rendering an in-memory Dart callback template cannot fail"),
        )
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn bridge_name(&self) -> &Identifier {
        &self.bridge_name
    }

    fn proxy_name(&self) -> &Identifier {
        &self.proxy_name
    }

    fn register_declaration(&self) -> &str {
        &self.register_declaration
    }

    fn create_declaration(&self) -> &str {
        &self.create_declaration
    }

    fn register_name(&self) -> &Identifier {
        &self.register_name
    }

    fn native_vtable(&self) -> &NativeVTable {
        &self.native_vtable
    }

    fn interface_methods(&self) -> &[String] {
        &self.interface_methods
    }

    fn proxy_methods(&self) -> &[String] {
        &self.proxy_methods
    }

    fn entries(&self) -> &[String] {
        &self.entries
    }

    fn callables(&self) -> &[String] {
        &self.callables
    }

    fn vtable_initializers(&self) -> &[String] {
        &self.vtable_initializers
    }

    fn free_vtable_initializer(&self) -> &str {
        &self.free_vtable_initializer
    }

    fn clone_vtable_initializer(&self) -> &str {
        &self.clone_vtable_initializer
    }

    fn shim_declarations(&self) -> &[String] {
        &self.shim_declarations
    }

    fn shim_register_call(&self) -> &str {
        &self.shim_register_call
    }

    fn shim_release_symbol(&self) -> &str {
        &self.shim_release_symbol
    }
}

/// `free`/`clone`'s Dart-side wiring: the fast-path pointer, the
/// listener-based declaration and its entry body, and the (type, value)
/// pair each contributes to the per-callback hooks registration call.
struct FreeCloneWiring {
    declarations: String,
    free_vtable_initializer: String,
    clone_vtable_initializer: String,
    register_arguments: String,
    register_argument_types: Vec<String>,
}

fn free_clone_wiring(vtable_name: &str) -> Result<FreeCloneWiring> {
    let type_name = super::shim::shim_type_name(vtable_name);
    let free_symbol = format!("{type_name}_free");
    let clone_symbol = format!("{type_name}_clone");

    let free_fast_ty = "$$ffi.Pointer<$$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64)>>";
    let free_listener_ty = "$$ffi.Pointer<$$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64, $$ffi.Pointer<$$ffi.Void>)>>";
    let clone_fast_ty = "$$ffi.Pointer<$$ffi.NativeFunction<$$ffi.Uint64 Function($$ffi.Uint64)>>";
    let clone_listener_ty = "$$ffi.Pointer<$$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64, $$ffi.Pointer<$$ffi.Void>, $$ffi.Pointer<$$ffi.Uint64>)>>";

    let free_symbol_fn_name = Identifier::parse(format!("_f${free_symbol}"))?;
    let clone_symbol_fn_name = Identifier::parse(format!("_f${clone_symbol}"))?;

    let declarations = format!(
        "static final _k$freeFast = $$ffi.Pointer.fromFunction<$$ffi.Void Function($$ffi.Uint64)>(_m$free);\n\
         @$$ffi.Native<$$ffi.Void Function($$ffi.Uint64)>(symbol: '{free_symbol}')\n\
         external static void {free_symbol_fn_name}(int handle);\n\
         static void _m$freeListenerEntry(int handle, $$ffi.Pointer<$$ffi.Void> gate) {{\n\
         \u{20}\u{20}_m$free(handle);\n\
         \u{20}\u{20}_f$signal_gate_ok(gate);\n\
         }}\n\
         static final _k$freeListener = $$ffi.NativeCallable<$$ffi.Void Function($$ffi.Uint64, $$ffi.Pointer<$$ffi.Void>)>.listener(_m$freeListenerEntry);\n\
         static final _k$cloneFast = $$ffi.Pointer.fromFunction<$$ffi.Uint64 Function($$ffi.Uint64)>(_m$clone, 0);\n\
         @$$ffi.Native<$$ffi.Uint64 Function($$ffi.Uint64)>(symbol: '{clone_symbol}')\n\
         external static int {clone_symbol_fn_name}(int handle);\n\
         static void _m$cloneListenerEntry(int handle, $$ffi.Pointer<$$ffi.Void> gate, $$ffi.Pointer<$$ffi.Uint64> out) {{\n\
         \u{20}\u{20}out.value = _m$clone(handle);\n\
         \u{20}\u{20}_f$signal_gate_ok(gate);\n\
         }}\n\
         static final _k$cloneListener = $$ffi.NativeCallable<$$ffi.Void Function($$ffi.Uint64, $$ffi.Pointer<$$ffi.Void>, $$ffi.Pointer<$$ffi.Uint64>)>.listener(_m$cloneListenerEntry);"
    );

    let free_vtable_initializer = format!(
        "..free = $$ffi.Native.addressOf<$$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64)>>({free_symbol_fn_name})"
    );
    let clone_vtable_initializer = format!(
        "..clone = $$ffi.Native.addressOf<$$ffi.NativeFunction<$$ffi.Uint64 Function($$ffi.Uint64)>>({clone_symbol_fn_name})"
    );

    Ok(FreeCloneWiring {
        declarations,
        free_vtable_initializer,
        clone_vtable_initializer,
        register_arguments: "_k$freeFast, _k$freeListener.nativeFunction, _k$cloneFast, _k$cloneListener.nativeFunction".to_owned(),
        register_argument_types: vec![
            free_fast_ty.to_owned(),
            free_listener_ty.to_owned(),
            clone_fast_ty.to_owned(),
            clone_listener_ty.to_owned(),
        ],
    })
}

/// The Dart `@Native` declarations for one callback's `_register`/
/// `_release` shim symbols.
fn shim_register_release_declaration(
    vtable_name: &str,
    free_clone: &FreeCloneWiring,
    methods: &[CallbackMethod],
) -> Result<String> {
    let argument_types = free_clone
        .register_argument_types
        .iter()
        .cloned()
        .chain(
            methods
                .iter()
                .filter_map(CallbackMethod::shim_register_argument_types)
                .flat_map(|(fast, listener)| [fast.to_owned(), listener.to_owned()]),
        )
        .collect::<Vec<_>>();

    let register_symbol = super::shim::register_symbol_name(vtable_name);
    let release_symbol = super::shim::release_symbol_name(vtable_name);

    let native_signature = format!(
        "$$ffi.Void Function($$ffi.Uint64, $$ffi.Size, {})",
        argument_types.join(", ")
    );
    // Parameter names, not bare types: an unnamed positional type in a
    // concrete Dart function declaration is a compile error. Never
    // referenced (the call site passes arguments positionally).
    let params = std::iter::once("int handle".to_owned())
        .chain(std::iter::once("int instanceHandle".to_owned()))
        .chain(
            argument_types
                .iter()
                .enumerate()
                .map(|(index, ty)| format!("{ty} p{index}")),
        )
        .collect::<Vec<_>>()
        .join(", ");
    let register_fn_name = Identifier::parse(format!("_f${register_symbol}"))?;
    let release_fn_name = Identifier::parse(format!("_f${release_symbol}"))?;

    Ok(format!(
        "@$$ffi.Native<{native_signature}>(symbol: '{register_symbol}')\nexternal static void {register_fn_name}({params});\n@$$ffi.Native<$$ffi.Void Function($$ffi.Uint64)>(symbol: '{release_symbol}')\nexternal static void {release_fn_name}(int handle);"
    ))
}

impl NativeVTable {
    fn from_protocol(protocol: &CCallback) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(format!("_$${}", protocol.vtable().name()))?,
            fields: protocol
                .vtable()
                .fields()
                .iter()
                .map(|field| {
                    Ok(NativeVTableField {
                        ty: TypeFragment::new(native::NativeType::from_c(field.ty())?.native()),
                        name: Identifier::parse(field.name())?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn fields(&self) -> &[NativeVTableField] {
        &self.fields
    }
}

impl NativeVTableField {
    fn ty(&self) -> &TypeFragment {
        &self.ty
    }

    fn name(&self) -> &Identifier {
        &self.name
    }
}

pub fn unsupported<T>(shape: &'static str) -> Result<T> {
    super::super::unsupported(shape)
}

fn broken<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}
