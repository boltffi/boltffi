use askama::Template as AskamaTemplate;
use boltffi_binding::{Bindings, Wasm32, WasmImports};

use crate::core::{
    FileLayout, FilePath, FilePlan, GeneratedFile, GeneratedOutput, RenderContext,
    RenderedDeclaration, Result, TextChunk,
};

use super::super::{name_style::ModuleName, syntax::StringLiteral};
use super::ClosureAdapter;
use super::Constant;

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/browser.ts", escape = "none")]
struct BrowserPreamble<'module> {
    runtime_package: &'module StringLiteral,
    imports_module: &'module StringLiteral,
    imports: &'module [StringLiteral],
    closure_adapters: &'module str,
    constant_initializers: &'module str,
}

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/node.ts", escape = "none")]
struct NodePreamble<'module> {
    runtime_package: &'module StringLiteral,
    imports_module: &'module StringLiteral,
    wasm_file: &'module StringLiteral,
    imports: &'module [StringLiteral],
    closure_adapters: &'module str,
}

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/node_init.ts", escape = "none")]
struct NodeInitialization<'module> {
    constant_initializers: &'module str,
}

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/imports.ts", escape = "none")]
struct ImportsModule<'module> {
    runtime_package: &'module StringLiteral,
}

pub struct Module<'module> {
    name: &'module ModuleName,
    runtime_package: &'module StringLiteral,
}

impl<'module> Module<'module> {
    pub fn new(name: &'module ModuleName, runtime_package: &'module StringLiteral) -> Self {
        Self {
            name,
            runtime_package,
        }
    }

    pub fn render<'decl>(
        &self,
        bindings: &Bindings<Wasm32>,
        context: &RenderContext<Wasm32>,
        declarations: Vec<RenderedDeclaration<'decl, Wasm32>>,
    ) -> Result<GeneratedOutput> {
        let imports_module = StringLiteral::new(&format!("./{}_imports.js", self.name));
        let wasm_imports = WasmImports::from_bindings(bindings);
        let imports = wasm_imports
            .iter()
            .map(|symbol| StringLiteral::new(symbol.name().as_str()))
            .collect::<Vec<_>>();
        let closure_adapters = ClosureAdapter::render_all(wasm_imports.closures(), context)?;
        let constant_initializers = Constant::initializers(&declarations, context)?;
        let browser = FileLayout::new()
            .with_file(
                FilePlan::all(FilePath::new(self.name.browser_path())?).with_preamble(
                    BrowserPreamble {
                        runtime_package: self.runtime_package,
                        imports_module: &imports_module,
                        imports: &imports,
                        closure_adapters: &closure_adapters,
                        constant_initializers: &constant_initializers,
                    }
                    .render()?,
                ),
            )
            .assemble_declarations(declarations.clone())?;
        let wasm_file = StringLiteral::new(&self.name.wasm_file());
        let node = FileLayout::new()
            .with_file(
                FilePlan::all(FilePath::new(self.name.node_path())?)
                    .with_preamble(
                        NodePreamble {
                            runtime_package: self.runtime_package,
                            imports_module: &imports_module,
                            wasm_file: &wasm_file,
                            imports: &imports,
                            closure_adapters: &closure_adapters,
                        }
                        .render()?,
                    )
                    .with_postamble(TextChunk::new(
                        NodeInitialization {
                            constant_initializers: &constant_initializers,
                        }
                        .render()?,
                    )),
            )
            .assemble_declarations(declarations)?;
        let imports = GeneratedOutput::new(vec![self.runtime_imports()?], Vec::new());
        Ok(GeneratedOutput::combine([browser, node, imports]))
    }

    pub fn runtime_imports(&self) -> Result<GeneratedFile> {
        Ok(GeneratedFile::new(
            FilePath::new(format!("{}_imports.ts", self.name))?,
            ImportsModule {
                runtime_package: self.runtime_package,
            }
            .render()?,
        ))
    }
}
