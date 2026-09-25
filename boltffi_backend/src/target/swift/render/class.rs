use askama::Template;
use boltffi_binding::{
    ClassDecl, ClassId, ConstantOwner, ExportedMethodDecl, HandlePresence, Native, NativeSymbol,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Diagnostic, Emitted, RenderContext, Result},
    target::swift::{
        SwiftHost,
        name_style::{GeneratedLocal, Name},
        render::{
            AssociatedConstants, Documentation, SwiftType,
            function::{AssociatedFunction, AssociatedFunctions, Initializer, Receiver},
        },
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/class.swift", escape = "none")]
struct ClassTemplate<'a> {
    class: &'a Class,
    constants: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    documentation: Documentation,
    name: TypeName,
    handle_type: TypeName,
    release: Identifier,
    constants: AssociatedConstants,
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
            constants: AssociatedConstants::from_owner(
                ConstantOwner::Class(declaration.id()),
                bridge,
                context,
            )?,
            initializers,
            static_methods,
            instance_methods,
            diagnostics,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = ClassTemplate {
            class: self,
            constants: self.constants.render()?,
        }
        .render()?;
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
        self.constants.requires_wire_runtime()
            || self
                .initializers
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassHandle {
    ty: TypeName,
    presence: HandlePresence,
}

impl ClassHandle {
    pub fn new(
        id: ClassId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            ty: SwiftType::class(id, context)?,
            presence,
        })
    }

    pub fn api_type(&self) -> TypeName {
        match self.presence {
            HandlePresence::Required => self.ty.clone(),
            HandlePresence::Nullable => self.ty.clone().optional(),
            _ => self.ty.clone(),
        }
    }

    pub fn parameter_argument(&self, value: Expression) -> Expression {
        match self.presence {
            HandlePresence::Required => Expression::member(value, "handle"),
            HandlePresence::Nullable => Expression::nil_coalescing(
                Expression::member(Expression::new(format!("{value}?")), "handle"),
                Self::empty(),
            ),
            _ => value,
        }
    }

    pub fn wrap(&self, handle: Expression) -> Expression {
        let wrapped = Expression::call(
            &self.ty,
            [Expression::labeled("handle", handle.clone())]
                .into_iter()
                .collect::<ArgumentList>(),
        );
        match self.presence {
            HandlePresence::Required => wrapped,
            HandlePresence::Nullable => Expression::conditional(
                Expression::equal(&handle, Self::empty()),
                Expression::nil(),
                wrapped,
            ),
            _ => wrapped,
        }
    }

    pub fn body(&self, handle: Expression, indent: &str) -> Result<String> {
        match self.presence {
            HandlePresence::Required => Ok(Statement::returns(self.wrap(handle)).indented(indent)),
            HandlePresence::Nullable => {
                let binding = GeneratedLocal::ReturnHandle.identifier()?;
                let value = Expression::identifier(binding.clone());
                Ok([
                    Statement::let_value(&binding, handle).indented(indent),
                    Statement::returns(self.wrap(value)).indented(indent),
                ]
                .join("\n"))
            }
            _ => Ok(Statement::returns(self.wrap(handle)).indented(indent)),
        }
    }

    pub fn factory_body(&self, handle: Expression, indent: &str) -> Result<String> {
        if self.presence != HandlePresence::Required {
            return Err(SwiftHost::unsupported("nullable class initializer"));
        }
        Ok(Statement::returns(Expression::call(
            "Self",
            [Expression::labeled("handle", handle)]
                .into_iter()
                .collect::<ArgumentList>(),
        ))
        .indented(indent))
    }

    fn empty() -> Expression {
        Expression::new("0")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedClassArgument {
    pub parameter: Identifier,
    pub local: Identifier,
    pub release: Identifier,
    pub presence: HandlePresence,
}

#[derive(Template)]
#[template(path = "target/swift/owned_call.swift", escape = "none")]
pub struct OwnedCallTemplate<'call> {
    pub arguments: Vec<&'call OwnedClassArgument>,
    pub invocation: Expression,
}
