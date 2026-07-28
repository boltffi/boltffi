use askama::Template;
use boltffi_binding::{CanonicalName, ClassDecl, ConstantOwner, Native};

use crate::{
    bridge::c::CBridgeContract,
    core::{AuxChunk, Diagnostic, Emitted, Error, HelperId, RenderContext, Result},
};

use super::super::{
    name_style::{Name, Namespace},
    syntax::{Identifier, Literal, TypeFragment},
};
use super::{AssociatedConstants, Documentation, Function, handle_carrier_type};

#[derive(Template)]
#[template(path = "target/csharp/class.cs", escape = "none")]
struct ClassTemplate<'class> {
    class: &'class Class,
    constants: &'class [String],
}

#[derive(Template)]
#[template(path = "target/csharp/class_release.cs", escape = "none")]
struct ClassReleaseTemplate<'class> {
    class: &'class Class,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::target::csharp) struct Class {
    documentation: Documentation,
    namespace: Namespace,
    name: Identifier,
    carrier_type: TypeFragment,
    release_name: Identifier,
    release_entry: Literal,
    release_helper_id: HelperId,
    constants: AssociatedConstants,
    initializers: Vec<ClassInitializer>,
    methods: Vec<Function>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClassInitializer {
    documentation: Documentation,
    function: Function,
    primary: bool,
}

impl Class {
    pub(in crate::target::csharp) fn from_declaration(
        declaration: &ClassDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(declaration.name()).pascal()?;
        let mut initializers = Vec::new();
        let mut methods = Vec::new();
        let mut diagnostics = Vec::new();
        for initializer in declaration.initializers() {
            match Function::from_class_initializer(
                initializer,
                declaration.id(),
                &name,
                declaration.handle(),
                Some(&namespace),
                bridge,
                context,
            ) {
                Ok((function, primary)) => {
                    let documentation =
                        Documentation::summary(initializer.meta().doc(), "        ");
                    let function = match primary {
                        true => function,
                        false => function.with_documentation(initializer.meta().doc()),
                    };
                    initializers.push(ClassInitializer {
                        documentation,
                        function,
                        primary,
                    });
                }
                Err(error) => {
                    collect_diagnostic(&mut diagnostics, "initializer", initializer.name(), error)?
                }
            }
        }
        for method in declaration.methods() {
            match Function::from_class_method(
                method,
                declaration.id(),
                &name,
                declaration.handle(),
                Some(&namespace),
                bridge,
                context,
            ) {
                Ok(function) => methods.push(function),
                Err(error) => collect_diagnostic(&mut diagnostics, "method", method.name(), error)?,
            }
        }
        validate_reserved_members(&name, &methods)?;
        let constants = AssociatedConstants::from_owner(
            ConstantOwner::Class(declaration.id()),
            &namespace,
            bridge,
            context,
        )?;
        Ok(Self {
            documentation: Documentation::summary(declaration.meta().doc(), "    "),
            namespace,
            name: name.clone(),
            carrier_type: handle_carrier_type(declaration.handle())?,
            release_name: Identifier::parse(format!("Native{name}Release"))?,
            release_entry: Literal::string(declaration.release().name().as_str()),
            release_helper_id: HelperId::new(CanonicalName::single(
                declaration.release().name().as_str(),
            )),
            constants,
            initializers,
            methods,
            diagnostics,
        })
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        let constants = self.constants.members()?;
        let emitted = Emitted::primary(
            ClassTemplate {
                class: self,
                constants: &constants,
            }
            .render()?,
        )
        .with_diagnostics(self.diagnostics.iter().cloned())
        .with_aux(AuxChunk::Helper {
            id: self.release_helper_id.clone(),
            text: ClassReleaseTemplate { class: self }.render()?.into(),
        });
        let emitted = self
            .initializers
            .iter()
            .map(|initializer| &initializer.function)
            .chain(self.methods.iter())
            .try_fold(emitted, |emitted, function| function.add_support(emitted))?;
        self.constants.add_support(emitted)
    }
}

/// Rejects methods that would duplicate a generated helper. The retain and
/// release helpers only collide at zero parameters; `RawHandle` is a property,
/// which C# rejects alongside a method of any arity.
fn validate_reserved_members(scope: &Identifier, methods: &[Function]) -> Result<()> {
    methods
        .iter()
        .find(|method| {
            method.name.as_str() == "RawHandle"
                || (method.parameters.is_empty()
                    && ["BoltffiRetain", "BoltffiRelease"].contains(&method.name.as_str()))
        })
        .map_or(Ok(()), |method| {
            Err(Error::CSharpNameCollision {
                scope: scope.to_string(),
                name: method.name.to_string(),
            })
        })
}

fn collect_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    kind: &'static str,
    name: &CanonicalName,
    error: Error,
) -> Result<()> {
    match error {
        Error::UnsupportedTarget { shape, .. } | Error::UnsupportedCAbi { shape } => {
            diagnostics.push(Diagnostic::new(format!(
                "{kind} {}: {shape}",
                Name::new(name).pascal()?
            )));
            Ok(())
        }
        error => Err(error),
    }
}
