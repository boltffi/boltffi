use askama::Template as AskamaTemplate;
use boltffi_binding::{ClassDecl, Native};

use crate::{
    bridge::{
        c::{Identifier, TypeFragment},
        python_cext::{ExtensionMethod, MethodFlags, MethodName, PythonCExtBridgeContract},
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::{
            codec,
            render::{argument, direct_vector, function, primitive, result},
        },
        name_style::Name,
        syntax::Identifier as PythonIdentifier,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/class_release.c", escape = "none")]
struct ReleaseTemplate {
    python_name: PythonIdentifier,
    wrapper: Identifier,
    storage: Identifier,
    handle_type: TypeFragment,
    parser: Identifier,
}

pub struct Class {
    release: Release,
    callables: Vec<function::Function>,
}

impl Class {
    pub fn from_declaration(
        declaration: &ClassDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::new(declaration)?;
        let initializers = declaration.initializers().iter().map(|initializer| {
            function::Function::from_export(
                symbols.initializer(initializer.name())?,
                initializer.symbol(),
                initializer.callable(),
                Vec::new(),
                bridge,
                context,
            )
        });
        let methods = declaration.methods().iter().map(|method| {
            let receiver = method
                .callable()
                .receiver()
                .map(|_| argument::Conversion::class_receiver(declaration.handle()))
                .transpose()?
                .into_iter()
                .collect();
            function::Function::from_export(
                symbols.method(method.name())?,
                method.target(),
                method.callable(),
                receiver,
                bridge,
                context,
            )
        });
        Ok(Self {
            release: Release::new(declaration, bridge)?,
            callables: initializers.chain(methods).collect::<Result<Vec<_>>>()?,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        let release = self.release.render()?;
        let callables = self
            .callables
            .into_iter()
            .map(function::Function::render)
            .collect::<Result<Vec<_>>>()?;
        let source = std::iter::once(release)
            .chain(
                callables
                    .into_iter()
                    .map(|emitted| emitted.primary_chunk().as_str().to_owned()),
            )
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Emitted::primary(source))
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        std::iter::once(self.release.method())
            .chain(self.callables.iter().flat_map(function::Function::methods))
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        self.callables
            .iter()
            .flat_map(function::Function::primitives)
            .chain(std::iter::once(self.release.primitive()))
            .collect()
    }

    pub fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.callables
            .iter()
            .flat_map(function::Function::owned_buffers)
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.callables
            .iter()
            .flat_map(function::Function::wire_primitives)
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.callables
            .iter()
            .flat_map(function::Function::direct_vector_elements)
    }

    pub fn native_sequences(&self) -> impl Iterator<Item = codec::NativeSequence> + '_ {
        self.callables
            .iter()
            .flat_map(function::Function::native_sequences)
    }

    pub fn has_string_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Function::has_string_argument)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Function::has_bytes_argument)
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Function::has_raw_wire_argument)
    }

    pub fn uses_async_protocol(&self) -> bool {
        self.callables
            .iter()
            .any(function::Function::uses_async_protocol)
    }
}

pub struct Symbols {
    class_name: PythonIdentifier,
    stem: String,
}

impl Symbols {
    pub fn new(declaration: &ClassDecl<Native>) -> Result<Self> {
        Ok(Self {
            class_name: PythonIdentifier::parse(Name::new(declaration.name()).class())?,
            stem: Name::new(declaration.name()).function_text()?,
        })
    }

    pub fn class_name(&self) -> &PythonIdentifier {
        &self.class_name
    }

    pub fn initializer(&self, name: &boltffi_binding::CanonicalName) -> Result<PythonIdentifier> {
        self.callable(name)
    }

    pub fn method(&self, name: &boltffi_binding::CanonicalName) -> Result<PythonIdentifier> {
        self.callable(name)
    }

    pub fn release(&self) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(format!("_boltffi_{}_release", self.stem))
    }

    fn callable(&self, name: &boltffi_binding::CanonicalName) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(format!(
            "_boltffi_{}_{}",
            self.stem,
            Name::new(name).function()?
        ))
    }
}

struct Release {
    python_name: PythonIdentifier,
    wrapper: Identifier,
    storage: Identifier,
    handle: primitive::Runtime,
    method: ExtensionMethod,
}

impl Release {
    fn new(declaration: &ClassDecl<Native>, bridge: &PythonCExtBridgeContract) -> Result<Self> {
        let symbols = Symbols::new(declaration)?;
        let python_name = symbols.release()?;
        let wrapper = Identifier::parse(format!(
            "boltffi_python_callable_wrapper_{}",
            declaration.release().name().as_str()
        ))?;
        let loaded =
            bridge
                .loaded_function(declaration.release())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "class release without C bridge symbol",
                })?;
        let method = ExtensionMethod::new(
            MethodName::parse(python_name.as_str())?,
            wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        Ok(Self {
            python_name,
            wrapper,
            storage: loaded.storage_name().clone(),
            handle: primitive::Runtime::native_handle(declaration.handle())?,
            method,
        })
    }

    fn render(self) -> Result<String> {
        Ok(ReleaseTemplate {
            python_name: self.python_name,
            wrapper: self.wrapper,
            storage: self.storage,
            handle_type: self.handle.c_type()?,
            parser: self.handle.parser()?,
        }
        .render()?)
    }

    fn method(&self) -> &ExtensionMethod {
        &self.method
    }

    fn primitive(&self) -> primitive::Runtime {
        self.handle
    }
}
