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

        let name = declaration_name(declaration.name())?;
        let methods = source_methods
            .iter()
            .zip(protocol.methods())
            .map(|(method, slot)| CallbackMethod::new(method, slot, &name, bridge, context))
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
