use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::Expression;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::cli::{CliError, Result};

use super::Error;

pub struct JavaScriptModules {
    root: PathBuf,
    visited: BTreeSet<PathBuf>,
}

impl JavaScriptModules {
    pub fn new(root: &Path) -> Result<Self> {
        let root = root.canonicalize().map_err(|source| CliError::ReadFailed {
            path: root.to_path_buf(),
            source,
        })?;
        Ok(Self {
            root,
            visited: BTreeSet::new(),
        })
    }

    pub fn validate(&mut self, specifier: &str) -> Result<()> {
        let mut pending = vec![self.resolve(&self.root, specifier)?];
        while let Some(path) = pending.pop() {
            if !self.visited.insert(path.clone()) {
                continue;
            }
            let source = fs::read_to_string(&path).map_err(|source| CliError::ReadFailed {
                path: path.clone(),
                source,
            })?;
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, &source, SourceType::mjs()).parse();
            if !parsed.errors.is_empty() {
                return Err(Error::Unsupported(format!(
                    "invalid JavaScript module {}: {:?}",
                    path.display(),
                    parsed.errors
                ))
                .into());
            }
            let directory = path
                .parent()
                .expect("a packaged JavaScript module has a parent");
            parsed
                .module_record
                .requested_modules
                .keys()
                .try_for_each(|specifier| {
                    pending.push(self.resolve(directory, specifier.as_str())?);
                    Ok::<_, CliError>(())
                })?;
            parsed.module_record.dynamic_imports.iter().try_for_each(|import| {
                let expression = Parser::new(&allocator, import.module_request.source_text(&source), SourceType::mjs())
                    .parse_expression().map_err(|error| Error::Unsupported(format!("invalid dynamic import in {}: {error:?}", path.display())))?;
                let Expression::StringLiteral(literal) = expression else {
                    return Err(Error::Unsupported(format!(
                        "dynamic import in {} must use a literal local module path so it can be packaged", path.display()
                    )).into());
                };
                pending.push(self.resolve(directory, literal.value.as_str())?);
                Ok::<_, CliError>(())
            })?;
        }
        Ok(())
    }

    fn resolve(&self, directory: &Path, specifier: &str) -> Result<PathBuf> {
        if !specifier.starts_with("./") && !specifier.starts_with("../") {
            return Err(Error::Unsupported(format!("JavaScript import {specifier:?} needs an external module resolver; only packaged local modules are supported")).into());
        }
        let path = directory.join(specifier);
        let canonical = path.canonicalize().map_err(|source| CliError::ReadFailed {
            path: path.clone(),
            source,
        })?;
        if !canonical.starts_with(&self.root) || !canonical.is_file() {
            return Err(Error::Unsupported(format!(
                "JavaScript import {specifier:?} escapes the packaged modules"
            ))
            .into());
        }
        Ok(canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::JavaScriptModules;
    use std::fs;

    #[test]
    fn packaged_modules_can_import_and_reexport_local_dependencies_with_cycles() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("main.js"), r#"import './shared.js'; export * from './other.js'; export const load = () => import('./lazy.js');"#).unwrap();
        fs::write(root.path().join("shared.js"), "import './main.js';").unwrap();
        fs::write(root.path().join("other.js"), "export const value = 42;").unwrap();
        fs::write(root.path().join("lazy.js"), "export const value = 73;").unwrap();
        JavaScriptModules::new(root.path())
            .unwrap()
            .validate("./main.js")
            .unwrap();
        fs::remove_file(root.path().join("lazy.js")).unwrap();
        assert!(
            JavaScriptModules::new(root.path())
                .unwrap()
                .validate("./main.js")
                .is_err()
        );
    }

    #[test]
    fn external_and_nonliteral_imports_are_not_silently_shipped() {
        let root = tempfile::tempdir().unwrap();
        [
            "import { value } from 'external-package';",
            "export * from 'external-package';",
            "export const load = () => import('external-package');",
            "export const load = (name) => import(name);",
        ]
        .into_iter()
        .for_each(|source| {
            fs::write(root.path().join("main.js"), source).unwrap();
            assert!(
                JavaScriptModules::new(root.path())
                    .unwrap()
                    .validate("./main.js")
                    .is_err(),
                "{source}"
            );
        });
    }

    #[test]
    fn strings_that_look_like_imports_are_ordinary_data() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("main.js"),
            r#"export const source = "import 'external-package';";"#,
        )
        .unwrap();
        JavaScriptModules::new(root.path())
            .unwrap()
            .validate("./main.js")
            .unwrap();
    }

    #[test]
    fn dependencies_cannot_escape_the_package() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("package")).unwrap();
        fs::write(root.path().join("outside.js"), "export const value = 42;").unwrap();
        fs::write(
            root.path().join("package/main.js"),
            "import '../outside.js';",
        )
        .unwrap();
        assert!(
            JavaScriptModules::new(&root.path().join("package"))
                .unwrap()
                .validate("./main.js")
                .is_err()
        );
    }
}
