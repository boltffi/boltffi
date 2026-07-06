use askama::Template;

use boltffi_binding::{
    ConstantValueDecl, DeclarationRef, ExportedCallable, IncomingParam, InitializerDecl,
    MethodDecl, Native, RustBody,
};

use crate::{
    core::{FileLayout, FilePath, FilePlan, GeneratedOutput, RenderedDeclaration, Result},
    target::swift::SwiftHost,
};

#[derive(Template)]
#[template(path = "target/swift/module.swift", escape = "none")]
struct ModuleTemplate<'a> {
    module: &'a str,
    closure_box: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ModuleFeatures {
    closure_box: bool,
}

pub struct Module<'host, 'decl> {
    host: &'host SwiftHost,
    declarations: Vec<RenderedDeclaration<'decl, Native>>,
}

impl ModuleFeatures {
    fn from_declarations(declarations: &[RenderedDeclaration<'_, Native>]) -> Self {
        Self {
            closure_box: declarations
                .iter()
                .any(|declaration| Self::needs_closure_box(declaration.declaration())),
        }
    }

    fn needs_closure_box(declaration: DeclarationRef<Native>) -> bool {
        match declaration {
            DeclarationRef::Record(record) => {
                Self::associated_functions_need_closure_box(record.initializers(), record.methods())
            }
            DeclarationRef::Enum(enumeration) => Self::associated_functions_need_closure_box(
                enumeration.initializers(),
                enumeration.methods(),
            ),
            DeclarationRef::Function(function) => {
                Self::callable_needs_closure_box(function.callable())
            }
            DeclarationRef::Class(class) => {
                Self::associated_functions_need_closure_box(class.initializers(), class.methods())
            }
            DeclarationRef::Constant(constant) => match constant.value() {
                ConstantValueDecl::Inline { .. } => false,
                ConstantValueDecl::Accessor { callable, .. } => {
                    Self::callable_needs_closure_box(callable)
                }
                _ => false,
            },
            DeclarationRef::Callback(_)
            | DeclarationRef::Stream(_)
            | DeclarationRef::CustomType(_) => false,
        }
    }

    fn associated_functions_need_closure_box<T>(
        initializers: &[InitializerDecl<Native>],
        methods: &[MethodDecl<Native, RustBody, T>],
    ) -> bool {
        initializers
            .iter()
            .map(InitializerDecl::callable)
            .chain(methods.iter().map(MethodDecl::callable))
            .any(Self::callable_needs_closure_box)
    }

    fn callable_needs_closure_box(callable: &ExportedCallable<Native>) -> bool {
        callable
            .params()
            .iter()
            .any(|parameter| matches!(parameter.payload(), IncomingParam::Closure(_)))
    }
}

impl<'host, 'decl> Module<'host, 'decl> {
    pub fn new(
        host: &'host SwiftHost,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Self {
        Self { host, declarations }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let features = ModuleFeatures::from_declarations(&self.declarations);
        let mut preamble = ModuleTemplate {
            module: self.host.module().as_str(),
            closure_box: features.closure_box,
        }
        .render()?
        .trim_end()
        .to_owned();
        preamble.push_str("\n\n");
        let file =
            FilePlan::all(FilePath::new(self.host.file_name().path())?).with_preamble(preamble);
        FileLayout::new()
            .with_file(file)
            .assemble_declarations(self.declarations)
    }
}
