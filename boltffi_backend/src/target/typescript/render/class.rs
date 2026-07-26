use askama::Template as AskamaTemplate;
use boltffi_binding::{ClassDecl, Wasm32};

use crate::core::{CoverageMode, Diagnostic, Emitted, Error, RenderContext, Result};

use super::super::{
    name_style::Name,
    syntax::{Identifier, MethodDeclaration, TypeName},
};
use super::Function;

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/class.ts", escape = "none")]
pub struct Class {
    name: TypeName,
    finalizer: Identifier,
    release: Identifier,
    methods: Vec<MethodDeclaration>,
    diagnostics: Vec<Diagnostic>,
}

impl Class {
    pub fn from_declaration(
        declaration: &ClassDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let name = Name::new(declaration.name()).type_name();
        let finalizer = Identifier::parse(format!("_{name}Finalizer"))?;
        let (methods, diagnostics) = declaration
            .initializers()
            .iter()
            .map(|initializer| {
                (
                    initializer.name(),
                    Function::from_class_initializer(initializer, context),
                    true,
                )
            })
            .chain(declaration.methods().iter().map(|method| {
                (
                    method.name(),
                    Function::from_class_method(method, &name, context),
                    method.callable().receiver().is_none(),
                )
            }))
            .try_fold(
                (Vec::new(), Vec::new()),
                |(mut rendered, mut diagnostics), (method_name, function, static_method)| {
                    match function {
                        Ok(function) => {
                            rendered.push(function.render_class_method(static_method)?);
                            Ok((rendered, diagnostics))
                        }
                        Err(Error::UnsupportedTarget { shape, .. })
                            if matches!(context.coverage_mode(), CoverageMode::Partial) =>
                        {
                            diagnostics.push(Diagnostic::new(format!(
                                "{}: {shape}",
                                method_name.as_path_string()
                            )));
                            Ok((rendered, diagnostics))
                        }
                        Err(error) => Err(error),
                    }
                },
            )?;
        Ok(Self {
            name,
            finalizer,
            release: Identifier::parse(declaration.release().name().as_str())?,
            methods,
            diagnostics,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?)
            .with_diagnostics(self.diagnostics.clone()))
    }
}
