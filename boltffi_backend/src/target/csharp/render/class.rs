use askama::Template;
use boltffi_binding::{CanonicalName, ClassDecl, Native};

use crate::{
    bridge::c::CBridgeContract,
    core::{AuxChunk, Diagnostic, Emitted, Error, HelperId, RenderContext, Result},
};

use super::super::{
    name_style::{Name, Namespace},
    syntax::{Identifier, Literal, TypeFragment},
};
use super::{Documentation, Function, handle_carrier_type};

#[derive(Template)]
#[template(path = "target/csharp/class.cs", escape = "none")]
struct ClassTemplate<'class> {
    class: &'class Class,
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
            initializers,
            methods,
            diagnostics,
        })
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        let mut emitted = Emitted::primary(ClassTemplate { class: self }.render()?)
            .with_diagnostics(self.diagnostics.iter().cloned())
            .with_aux(AuxChunk::Helper {
                id: self.release_helper_id.clone(),
                text: ClassReleaseTemplate { class: self }.render()?.into(),
            });
        for function in self
            .initializers
            .iter()
            .map(|initializer| &initializer.function)
            .chain(self.methods.iter())
        {
            let (_, aux, diagnostics) = function.render()?.into_parts();
            for chunk in aux {
                emitted = emitted.with_aux(chunk);
            }
            emitted = emitted.with_diagnostics(diagnostics);
        }
        Ok(emitted)
    }
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
