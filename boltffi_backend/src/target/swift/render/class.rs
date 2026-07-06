use askama::Template;
use boltffi_binding::{ClassDecl, ExportedMethodDecl, Native, NativeSymbol};

use crate::{
    bridge::c::CBridgeContract,
    core::{Diagnostic, Emitted, RenderContext, Result},
    target::swift::{
        name_style::Name,
        render::{
            Documentation, SwiftType,
            function::{AssociatedFunction, AssociatedFunctions, Initializer, Receiver},
        },
        syntax::{Identifier, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/class.swift", escape = "none")]
struct ClassTemplate<'a> {
    class: &'a Class,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    documentation: Documentation,
    name: TypeName,
    handle_type: TypeName,
    release: Identifier,
    initializers: Vec<Initializer>,
    static_methods: Vec<AssociatedFunction>,
    instance_methods: Vec<AssociatedFunction>,
    diagnostics: Vec<Diagnostic>,
}

impl Class {
    pub fn from_declaration(
        declaration: &ClassDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let (initializers, mut diagnostics) =
            Initializer::from_class_declarations(declaration.initializers(), bridge, context)?
                .into_parts();
        let (static_methods, static_diagnostics) =
            Self::methods(declaration.methods(), None, bridge, context)?.into_parts();
        let (instance_methods, instance_diagnostics) = Self::methods(
            declaration.methods(),
            Some(Receiver::class_handle()),
            bridge,
            context,
        )?
        .into_parts();
        diagnostics.extend(static_diagnostics);
        diagnostics.extend(instance_diagnostics);
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc(), ""),
            name: Name::new(declaration.name()).type_name(),
            handle_type: SwiftType::handle_carrier(declaration.handle())?,
            release: Identifier::parse(declaration.release().name().as_str())?,
            initializers,
            static_methods,
            instance_methods,
            diagnostics,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = ClassTemplate { class: self }.render()?;
        source.push_str("\n\n");
        let emitted = Emitted::primary(source).with_diagnostics(self.diagnostics.clone());
        let emitted = match self.requires_wire_runtime() {
            true => emitted.with_aux(AssociatedFunction::wire_helper()?),
            false => emitted,
        };
        let emitted = match self.requires_async_runtime() {
            true => emitted.with_aux(AssociatedFunction::async_helper()?),
            false => emitted,
        };
        Ok(emitted)
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &TypeName {
        &self.name
    }

    fn release(&self) -> &Identifier {
        &self.release
    }

    fn handle_type(&self) -> &TypeName {
        &self.handle_type
    }

    fn initializers(&self) -> &[Initializer] {
        &self.initializers
    }

    fn static_methods(&self) -> &[AssociatedFunction] {
        &self.static_methods
    }

    fn instance_methods(&self) -> &[AssociatedFunction] {
        &self.instance_methods
    }

    fn requires_wire_runtime(&self) -> bool {
        self.initializers
            .iter()
            .any(Initializer::requires_wire_runtime)
            || self
                .static_methods
                .iter()
                .chain(self.instance_methods.iter())
                .any(AssociatedFunction::requires_wire_runtime)
    }

    fn requires_async_runtime(&self) -> bool {
        self.static_methods
            .iter()
            .chain(self.instance_methods.iter())
            .any(AssociatedFunction::requires_async_runtime)
    }

    fn methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Option<Receiver>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<AssociatedFunctions> {
        AssociatedFunction::from_methods(methods, receiver, bridge, context)
    }
}
