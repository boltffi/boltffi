use std::collections::BTreeMap;

use semver::Version;
use serde::Deserialize;
use wasmparser::{BinaryReader, ExternalKind, FuncType, Import, Parser, Payload, TypeRef};

use super::Error;

pub struct Module<'wasm> {
    pub imports: Vec<Import<'wasm>>,
    pub bindgen_version: Option<Version>,
    exports: BTreeMap<&'wasm str, FuncType>,
    environment: BTreeMap<&'wasm str, FuncType>,
    shared_memory: bool,
    exception_tags: bool,
    exports_memory: bool,
}

impl<'wasm> Module<'wasm> {
    pub fn parse(bytes: &'wasm [u8]) -> Result<Self, Error> {
        let mut imports = Vec::new();
        let mut types = Vec::new();
        let mut functions = Vec::new();
        let mut export_indices = Vec::new();
        let mut bindgen_version = None;
        let mut shared_memory = false;
        let mut exception_tags = false;
        let mut exports_memory = false;
        Parser::new(0).parse_all(bytes).try_for_each(|payload| {
            match payload? {
                Payload::TypeSection(section) => {
                    types = section
                        .into_iter_err_on_gc_types()
                        .collect::<Result<_, _>>()?;
                }
                Payload::ImportSection(section) => {
                    section.into_imports().try_for_each(|import| {
                        let import = import?;
                        match import.ty {
                            TypeRef::Func(index) | TypeRef::FuncExact(index) => {
                                functions.push(index)
                            }
                            TypeRef::Memory(memory) => shared_memory |= memory.shared,
                            TypeRef::Tag(_) => exception_tags = true,
                            _ => {}
                        }
                        imports.push(import);
                        Ok::<_, Error>(())
                    })?;
                }
                Payload::FunctionSection(section) => {
                    functions.extend(section.into_iter().collect::<Result<Vec<_>, _>>()?);
                }
                Payload::ExportSection(section) => {
                    section.into_iter().try_for_each(|export| {
                        let export = export?;
                        exports_memory |=
                            export.kind == ExternalKind::Memory && export.name == "memory";
                        if export.kind == ExternalKind::Func
                            && (export.name.starts_with("boltffi_")
                                || export.name.starts_with("__boltffi_"))
                        {
                            export_indices.push((export.name, export.index));
                        }
                        Ok::<_, Error>(())
                    })?;
                }
                Payload::MemorySection(section) => {
                    section.into_iter().try_for_each(|memory| {
                        shared_memory |= memory?.shared;
                        Ok::<_, Error>(())
                    })?;
                }
                Payload::TagSection(section) => exception_tags |= section.count() != 0,
                Payload::CustomSection(section) if section.name() == "__wasm_bindgen_unstable" => {
                    Self::read_versions(section.data(), &mut bindgen_version)?;
                }
                _ => {}
            }
            Ok::<_, Error>(())
        })?;
        let function_type = |index: u32| {
            types
                .get(index as usize)
                .cloned()
                .ok_or(Error::MissingFunctionType { index })
        };
        let exports = export_indices
            .into_iter()
            .map(|(name, index)| {
                let type_index =
                    functions
                        .get(index as usize)
                        .ok_or_else(|| Error::MissingExportFunction {
                            name: name.to_owned(),
                            index,
                        })?;
                Ok((name, function_type(*type_index)?))
            })
            .collect::<Result<_, Error>>()?;
        let environment = imports
            .iter()
            .filter(|import| import.module == "env")
            .filter_map(|import| match import.ty {
                TypeRef::Func(index) | TypeRef::FuncExact(index) => Some((import.name, index)),
                _ => None,
            })
            .map(|(name, index)| Ok((name, function_type(index)?)))
            .collect::<Result<_, Error>>()?;
        if bindgen_version.is_none()
            && let Some(import) = imports
                .iter()
                .find(|import| import.module.starts_with("__wbindgen"))
        {
            return Err(Error::MissingMetadata {
                module: import.module.to_owned(),
                name: import.name.to_owned(),
            });
        }
        Ok(Self {
            imports,
            bindgen_version,
            exports,
            environment,
            shared_memory,
            exception_tags,
            exports_memory,
        })
    }

    pub fn validate_bindgen(&self) -> Result<(), Error> {
        if self.shared_memory {
            return Err(Error::Unsupported(
                "wasm-bindgen with shared memory requires a thread-aware loader".to_owned(),
            ));
        }
        if self.exception_tags {
            return Err(Error::Unsupported("wasm-bindgen exception-handling builds require an unwind-aware loader; use panic=abort".to_owned()));
        }
        Ok(())
    }

    pub fn validate_interface(&self, processed: &Self) -> Result<(), Error> {
        if self.exports_memory && !processed.exports_memory {
            return Err(Error::ExportChanged {
                name: "memory".to_owned(),
            });
        }
        self.exports.iter().try_for_each(|(name, signature)| {
            if processed.exports.get(name) == Some(signature) {
                Ok(())
            } else {
                Err(Error::ExportChanged {
                    name: (*name).to_owned(),
                })
            }
        })?;
        processed
            .environment
            .iter()
            .try_for_each(|(name, signature)| {
                if self.environment.get(name) == Some(signature) {
                    Ok(())
                } else {
                    Err(Error::EnvironmentImportChanged {
                        name: (*name).to_owned(),
                    })
                }
            })
    }

    fn read_versions(bytes: &[u8], version: &mut Option<Version>) -> Result<(), Error> {
        #[derive(Deserialize)]
        struct Metadata {
            version: Version,
        }
        if bytes.is_empty() {
            return Err(Error::Metadata("empty metadata section".to_owned()));
        }
        let mut reader = BinaryReader::new(bytes, 0);
        std::iter::from_fn(|| {
            (!reader.eof()).then(|| {
                let length = reader.read_u32()? as usize;
                let header = reader.read_bytes(length)?;
                let length = reader.read_u32()? as usize;
                reader.read_bytes(length)?;
                Ok::<_, Error>(header)
            })
        })
        .try_for_each(|header| {
            let metadata: Metadata = serde_json::from_slice(header?)
                .map_err(|error| Error::Metadata(error.to_string()))?;
            if version
                .as_ref()
                .is_some_and(|previous| *previous != metadata.version)
            {
                return Err(Error::Metadata(
                    "multiple wasm-bindgen versions in one module".to_owned(),
                ));
            }
            *version = Some(metadata.version);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Module};
    use semver::Version;

    fn metadata_module(versions: &[&str]) -> Vec<u8> {
        let sections = versions
            .iter()
            .map(|version| {
                let header = format!(r#"{{"schema_version":"test","version":"{version}"}}"#);
                let payload = (header.len() as u32)
                    .to_le_bytes()
                    .into_iter()
                    .chain(header.bytes())
                    .chain(0u32.to_le_bytes())
                    .map(|byte| format!("\\{byte:02x}"))
                    .collect::<String>();
                format!(r#"(@custom "__wasm_bindgen_unstable" "{payload}")"#)
            })
            .collect::<String>();
        wat::parse_str(format!("(module {sections})")).unwrap()
    }

    #[test]
    fn dependency_metadata_selects_the_exact_cli_version() {
        let bytes = metadata_module(&["0.2.129", "0.2.129"]);
        let module = Module::parse(&bytes).unwrap();
        assert_eq!(module.bindgen_version, Some(Version::new(0, 2, 129)));
        let mixed = metadata_module(&["0.2.122", "0.2.129"]);
        assert!(matches!(Module::parse(&mixed), Err(Error::Metadata(_))));
    }

    #[test]
    fn truncated_metadata_cannot_be_treated_as_a_pure_module() {
        let bytes =
            wat::parse_str(r#"(module (@custom "__wasm_bindgen_unstable" "\ff\ff\ff\ff"))"#)
                .unwrap();
        assert!(Module::parse(&bytes).is_err());
    }

    #[test]
    fn stripped_bindgen_imports_require_a_fresh_cargo_artifact() {
        let bytes =
            wat::parse_str(r#"(module (import "__wbindgen_placeholder__" "__wbg_random" (func)))"#)
                .unwrap();
        assert!(
            matches!(Module::parse(&bytes), Err(Error::MissingMetadata { name, .. }) if name == "__wbg_random")
        );
    }

    #[test]
    fn pure_boltffi_does_not_require_a_bindgen_cli() {
        let bytes = wat::parse_str(r#"(module (func (export "boltffi_add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add))"#).unwrap();
        let module = Module::parse(&bytes).unwrap();
        assert!(module.bindgen_version.is_none());
        module.validate_interface(&module).unwrap();
    }

    #[test]
    fn processing_must_preserve_export_and_environment_signatures() {
        let original = wat::parse_str(r#"(module (import "env" "callback" (func (param i32))) (func (export "boltffi_add") (result i64) i64.const 42))"#).unwrap();
        let changed_export = wat::parse_str(r#"(module (import "env" "callback" (func (param i32))) (func (export "boltffi_add") (result i32) i32.const 42))"#).unwrap();
        let changed_import = wat::parse_str(r#"(module (import "env" "callback" (func (param i64))) (func (export "boltffi_add") (result i64) i64.const 42))"#).unwrap();
        let original = Module::parse(&original).unwrap();
        assert!(
            matches!(original.validate_interface(&Module::parse(&changed_export).unwrap()), Err(Error::ExportChanged { name }) if name == "boltffi_add")
        );
        assert!(
            matches!(original.validate_interface(&Module::parse(&changed_import).unwrap()), Err(Error::EnvironmentImportChanged { name }) if name == "callback")
        );
    }

    #[test]
    fn shared_memory_and_unwind_builds_are_rejected() {
        let shared = wat::parse_str("(module (memory 1 2 shared))").unwrap();
        let unwinding = wat::parse_str("(module (tag (param externref)))").unwrap();
        assert!(
            matches!(Module::parse(&shared).unwrap().validate_bindgen(), Err(Error::Unsupported(message)) if message.contains("shared memory"))
        );
        assert!(
            matches!(Module::parse(&unwinding).unwrap().validate_bindgen(), Err(Error::Unsupported(message)) if message.contains("panic=abort"))
        );
    }

    #[test]
    fn processing_preserves_local_callback_exports() {
        let original = wat::parse_str(r#"(module (func (export "__boltffi_local_demo_listener_on_event") (param i32) (result i32) local.get 0))"#).unwrap();
        let original = Module::parse(&original).unwrap();
        [
            r#"(module)"#,
            r#"(module (func (export "__boltffi_local_demo_listener_on_event") (param i64) (result i64) local.get 0))"#,
        ]
        .into_iter()
        .for_each(|processed| {
            let processed = wat::parse_str(processed).unwrap();
            assert!(matches!(original.validate_interface(&Module::parse(&processed).unwrap()), Err(Error::ExportChanged { name }) if name == "__boltffi_local_demo_listener_on_event"));
        });
        original.validate_interface(&original).unwrap();
    }
}
