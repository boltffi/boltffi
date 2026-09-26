use std::collections::{HashMap, HashSet};

use boltffi_ast::{
    ClassDef, ConstantDef, ConstantOwner, EnumDef, EnumId, Path, PathRoot, RecordDef, RecordId,
    SourceContract, StreamDef, TraitDef,
};
use boltffi_binding::{Native, SurfaceLower, Wasm32};
use proc_macro2::TokenStream;
#[cfg(test)]
use quote::format_ident;
use quote::quote;
use syn::{Path as RustPath, Type};

use crate::expansion::{contract::Expansion, error::Error, rust_api, wrapper};

pub struct Expander<'lowered> {
    source: &'lowered SourceContract,
    support: &'lowered SourceContract,
    visible_paths: HashMap<String, Path>,
    selected: Option<HashSet<String>>,
}

struct SurfaceExpander<'expansion, 'lowered> {
    source: &'lowered SourceContract,
    support: &'lowered SourceContract,
    visible_paths: &'expansion HashMap<String, Path>,
    selected: Option<&'expansion HashSet<String>>,
    expansion: ExpansionSurface<'expansion, 'lowered>,
}

struct StreamOwner<'source> {
    declaration: &'source ClassDef,
    rust_type: RustPath,
}

#[derive(Clone, Copy)]
enum ExpansionSurface<'expansion, 'lowered> {
    Native(&'expansion Expansion<'lowered, Native>),
    Wasm32(&'expansion Expansion<'lowered, Wasm32>),
}

impl<'expansion, 'lowered> SurfaceExpander<'expansion, 'lowered> {
    const fn native(
        source: &'lowered SourceContract,
        support: &'lowered SourceContract,
        visible_paths: &'expansion HashMap<String, Path>,
        expansion: &'expansion Expansion<'lowered, Native>,
    ) -> Self {
        Self {
            source,
            support,
            visible_paths,
            selected: None,
            expansion: ExpansionSurface::Native(expansion),
        }
    }

    const fn wasm32(
        source: &'lowered SourceContract,
        support: &'lowered SourceContract,
        visible_paths: &'expansion HashMap<String, Path>,
        expansion: &'expansion Expansion<'lowered, Wasm32>,
    ) -> Self {
        Self {
            source,
            support,
            visible_paths,
            selected: None,
            expansion: ExpansionSurface::Wasm32(expansion),
        }
    }

    const fn selecting(mut self, selected: Option<&'expansion HashSet<String>>) -> Self {
        self.selected = selected;
        self
    }

    fn selected(&self, id: &str) -> bool {
        self.selected.is_none_or(|selected| selected.contains(id))
    }

    fn record_and_enum_imports(&self) -> Result<Vec<TokenStream>, Error> {
        if self.selected.is_some() {
            return Ok(Vec::new());
        }
        let package = self.source.package.name.replace('-', "_");
        self.source
            .records
            .iter()
            .map(|record| record.id.as_str())
            .chain(
                self.source
                    .enums
                    .iter()
                    .map(|enumeration| enumeration.id.as_str()),
            )
            .filter(|id| id.split("::").next() == Some(package.as_str()))
            .filter_map(|id| self.visible_paths.get(id))
            .map(|path| {
                let path = Self::path_tokens(path)?;
                Ok(quote! {
                    #[allow(unused_imports)]
                    use #path;
                })
            })
            .collect()
    }

    fn trait_path(&self, source: &TraitDef) -> Result<Option<TokenStream>, Error> {
        self.visible_paths
            .get(source.id.as_str())
            .map(Self::path_tokens)
            .transpose()
    }

    fn trait_object_impls(&self, source: &TraitDef) -> bool {
        let package = self.source.package.name.replace('-', "_");
        source.id.as_str().split("::").next() == Some(package.as_str())
    }

    fn path_tokens(path: &Path) -> Result<TokenStream, Error> {
        let prefix = match path.root {
            PathRoot::Relative => TokenStream::new(),
            PathRoot::Crate => quote! { crate:: },
            PathRoot::Self_ => quote! { self:: },
            PathRoot::Super(levels) => {
                let parents =
                    std::iter::repeat_n(quote! { super }, levels.get()).collect::<Vec<_>>();
                quote! { #(#parents)::*:: }
            }
            PathRoot::Absolute => quote! { :: },
        };
        let segments = path
            .segments
            .iter()
            .map(|segment| {
                if !segment.arguments.is_empty() {
                    return Err(Error::UnsupportedExpansion("generic visible path"));
                }
                syn::parse_str::<syn::Ident>(segment.name.as_str()).map_err(|_| {
                    Error::SourceSyntaxMismatch("callback trait path is not Rust syntax")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote! { #prefix #(#segments)::* })
    }

    fn path_type(path: &Path) -> Result<Type, Error> {
        syn::parse2(Self::path_tokens(path)?)
            .map_err(|_| Error::SourceSyntaxMismatch("visible path is not a Rust type"))
    }

    fn source_record(&self, record: &RecordDef) -> bool {
        self.source
            .records
            .iter()
            .any(|source| source.id == record.id)
    }

    fn source_enumeration(&self, enumeration: &EnumDef) -> bool {
        self.source
            .enums
            .iter()
            .any(|source| source.id == enumeration.id)
    }

    fn support_type(&self, id: &str) -> Result<Type, Error> {
        self.visible_paths
            .get(id)
            .ok_or(Error::UnsupportedExpansion(
                "dependency data impl target is not visible",
            ))
            .and_then(Self::path_type)
    }

    /// The declaration's type as its own invocation site names it.
    fn declared_type(&self, id: &str) -> Result<Type, Error> {
        if self.selected.is_none() {
            return self.root_type(id);
        }
        let name = id
            .rsplit("::")
            .next()
            .ok_or(Error::SourceSyntaxMismatch("declaration id is empty"))?;
        syn::parse_str(name)
            .map_err(|_| Error::SourceSyntaxMismatch("declaration name is not a Rust type"))
    }

    fn root_type(&self, id: &str) -> Result<Type, Error> {
        let package = self.source.package.name.replace('-', "_");
        let mut path = id.split("::");
        if path.next() != Some(package.as_str()) {
            return Err(Error::SourceSyntaxMismatch(
                "source data declaration belongs to another crate",
            ));
        }
        let path = path
            .map(|segment| {
                syn::parse_str::<syn::Ident>(segment)
                    .map_err(|_| Error::SourceSyntaxMismatch("source data path is not Rust syntax"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        syn::parse2(quote! { crate::#(#path)::* })
            .map_err(|_| Error::SourceSyntaxMismatch("source data path is not a Rust type"))
    }

    fn stream_owner(&self, stream: &StreamDef) -> Result<Option<StreamOwner<'lowered>>, Error> {
        let Some(owner_id) = stream.owner.as_ref() else {
            return Ok(None);
        };
        let declaration = self
            .support
            .classes
            .iter()
            .find(|class| &class.id == owner_id)
            .ok_or(Error::SourceSyntaxMismatch("stream owner class is missing"))?;
        let rust_type = self.owner_path(owner_id.as_str())?;
        Ok(Some(StreamOwner {
            declaration,
            rust_type,
        }))
    }

    fn constant_owner(
        &self,
        constant: &ConstantDef,
    ) -> Result<Option<(rust_api::CallableOwner<'lowered>, RustPath)>, Error> {
        let Some(owner) = constant.owner.as_ref() else {
            return Ok(None);
        };
        let owner_id = owner.as_str();
        let rust_type = self.owner_path(owner_id)?;
        let owner = match owner {
            ConstantOwner::Record(id) => self
                .support
                .records
                .iter()
                .find(|record| &record.id == id)
                .map(rust_api::CallableOwner::Record),
            ConstantOwner::Enum(id) => self
                .support
                .enums
                .iter()
                .find(|enumeration| &enumeration.id == id)
                .map(rust_api::CallableOwner::Enum),
            ConstantOwner::Class(id) => self
                .support
                .classes
                .iter()
                .find(|class| &class.id == id)
                .map(rust_api::CallableOwner::Class),
        }
        .ok_or(Error::SourceSyntaxMismatch(
            "associated constant owner declaration is missing",
        ))?;
        Ok(Some((owner, rust_type)))
    }

    fn owner_path(&self, id: &str) -> Result<RustPath, Error> {
        if let Some(path) = self.visible_paths.get(id) {
            return syn::parse2(Self::path_tokens(path)?)
                .map_err(|_| Error::SourceSyntaxMismatch("visible owner path is not Rust syntax"));
        }
        let package = self.source.package.name.replace('-', "_");
        if id.split("::").next() != Some(package.as_str()) {
            return Err(Error::UnsupportedExpansion(
                "dependency callable owner is not visible",
            ));
        }
        Self::source_path(id)
    }

    fn source_path(id: &str) -> Result<RustPath, Error> {
        let name = id
            .rsplit("::")
            .next()
            .ok_or(Error::SourceSyntaxMismatch("callable owner id is empty"))?;
        syn::parse_str(name)
            .map_err(|_| Error::SourceSyntaxMismatch("callable owner name is not a Rust path"))
    }
}

fn owner_id(owner: &ConstantOwner) -> &str {
    match owner {
        ConstantOwner::Record(id) => id.as_str(),
        ConstantOwner::Enum(id) => id.as_str(),
        ConstantOwner::Class(id) => id.as_str(),
    }
}

impl<'lowered> Expander<'lowered> {
    #[cfg(test)]
    pub fn new(source: &'lowered SourceContract) -> Self {
        Self {
            source,
            support: source,
            visible_paths: HashMap::new(),
            selected: None,
        }
    }

    /// Expands only the declarations one macro invocation owns, resolving
    /// everything they reference against the rest of `contract`.
    pub fn invocation(
        contract: &'lowered SourceContract,
        selected: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            source: contract,
            support: contract,
            visible_paths: HashMap::new(),
            selected: Some(selected.into_iter().collect()),
        }
    }

    pub fn native(&self, expansion: &Expansion<'lowered, Native>) -> Result<TokenStream, Error> {
        let wrappers =
            SurfaceExpander::native(self.source, self.support, &self.visible_paths, expansion)
                .selecting(self.selected.as_ref())
                .expand()?;
        Ok(wrappers)
    }

    pub fn wasm32(&self, expansion: &Expansion<'lowered, Wasm32>) -> Result<TokenStream, Error> {
        let wrappers =
            SurfaceExpander::wasm32(self.source, self.support, &self.visible_paths, expansion)
                .selecting(self.selected.as_ref())
                .expand()?;
        Ok(wrappers)
    }

    pub fn record_runtime<S: SurfaceLower>(
        &self,
        id: &RecordId,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error> {
        let source = self
            .source
            .records
            .iter()
            .find(|record| &record.id == id)
            .ok_or(Error::WrongDeclaration)?;
        wrapper::record::Record::new(expansion.record(source)?, expansion).render_runtime()
    }

    pub fn enumeration_runtime<S: SurfaceLower>(
        &self,
        id: &EnumId,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error> {
        let source = self
            .source
            .enums
            .iter()
            .find(|enumeration| &enumeration.id == id)
            .ok_or(Error::WrongDeclaration)?;
        wrapper::enumeration::Enumeration::new(expansion.enumeration(source)?, expansion)
            .render_runtime()
    }

    #[cfg(test)]
    pub fn all(
        &self,
        native: &Expansion<'lowered, Native>,
        wasm32: &Expansion<'lowered, Wasm32>,
    ) -> Result<TokenStream, Error> {
        let native = Self::surface_module(
            format_ident!("__boltffi_native"),
            quote! { #[cfg(not(target_arch = "wasm32"))] },
            self.native(native)?,
        );
        let wasm32 = Self::surface_module(
            format_ident!("__boltffi_wasm32"),
            quote! { #[cfg(target_arch = "wasm32")] },
            self.wasm32(wasm32)?,
        );

        Ok(quote! {
            #native
            #wasm32
        })
    }

    #[cfg(test)]
    fn surface_module(
        module: proc_macro2::Ident,
        cfg: TokenStream,
        tokens: TokenStream,
    ) -> TokenStream {
        quote! {
            #cfg
            mod #module {
                use super::*;

                #tokens
            }
        }
    }
}

impl<'expansion, 'lowered> SurfaceExpander<'expansion, 'lowered> {
    fn expand(self) -> Result<TokenStream, Error> {
        let imports = self.record_and_enum_imports()?;
        let callbacks = self.callbacks()?;
        let records = self.records()?;
        let enumerations = self.enumerations()?;
        let classes = self.classes()?;
        let streams = self.streams()?;
        let constants = self.constants()?;
        let functions = self.functions()?;

        Ok(quote! {
            #(#imports)*
            #(#callbacks)*
            #(#records)*
            #(#enumerations)*
            #(#classes)*
            #(#streams)*
            #(#constants)*
            #(#functions)*
        })
    }

    fn callbacks(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .traits
            .iter()
            .filter(|source| self.selected(source.id.as_str()))
            .map(|source| match self.expansion {
                ExpansionSurface::Native(expansion) => {
                    wrapper::callback::Trait::new(expansion.callback_trait(source)?, expansion)
                        .with_path(self.trait_path(source)?, self.trait_object_impls(source))
                        .render()
                }
                ExpansionSurface::Wasm32(expansion) => {
                    wrapper::callback::Trait::new(expansion.callback_trait(source)?, expansion)
                        .with_path(self.trait_path(source)?, self.trait_object_impls(source))
                        .render()
                }
            })
            .collect()
    }

    fn records(&self) -> Result<Vec<TokenStream>, Error> {
        let mut records = self
            .source
            .records
            .iter()
            .filter(|source| self.selected(source.id.as_str()))
            .map(|source| {
                let rust_type = self.declared_type(source.id.as_str())?;
                match self.expansion {
                    ExpansionSurface::Native(expansion) => {
                        wrapper::record::Record::new(expansion.record(source)?, expansion)
                            .render_exports(rust_type)
                    }
                    ExpansionSurface::Wasm32(expansion) => {
                        wrapper::record::Record::new(expansion.record(source)?, expansion)
                            .render_exports(rust_type)
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if self.selected.is_none() {
            records.extend(self.support_records()?);
        }
        Ok(records)
    }

    fn enumerations(&self) -> Result<Vec<TokenStream>, Error> {
        let mut enumerations = self
            .source
            .enums
            .iter()
            .filter(|source| self.selected(source.id.as_str()))
            .map(|source| {
                let rust_type = self.declared_type(source.id.as_str())?;
                match self.expansion {
                    ExpansionSurface::Native(expansion) => wrapper::enumeration::Enumeration::new(
                        expansion.enumeration(source)?,
                        expansion,
                    )
                    .render_exports(rust_type),
                    ExpansionSurface::Wasm32(expansion) => wrapper::enumeration::Enumeration::new(
                        expansion.enumeration(source)?,
                        expansion,
                    )
                    .render_exports(rust_type),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if self.selected.is_none() {
            enumerations.extend(self.support_enumerations()?);
        }
        Ok(enumerations)
    }

    fn support_records(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .records
            .iter()
            .filter(|source| !source.methods.is_empty())
            .filter(|source| !self.source_record(source))
            .map(|source| {
                let rust_type = self.support_type(source.id.as_str())?;
                match self.expansion {
                    ExpansionSurface::Native(expansion) => {
                        wrapper::record::Record::new(expansion.record(source)?, expansion)
                            .render_exports(rust_type)
                    }
                    ExpansionSurface::Wasm32(expansion) => {
                        wrapper::record::Record::new(expansion.record(source)?, expansion)
                            .render_exports(rust_type)
                    }
                }
            })
            .collect()
    }

    fn support_enumerations(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .enums
            .iter()
            .filter(|source| !source.methods.is_empty())
            .filter(|source| !self.source_enumeration(source))
            .map(|source| {
                let rust_type = self.support_type(source.id.as_str())?;
                match self.expansion {
                    ExpansionSurface::Native(expansion) => wrapper::enumeration::Enumeration::new(
                        expansion.enumeration(source)?,
                        expansion,
                    )
                    .render_exports(rust_type),
                    ExpansionSurface::Wasm32(expansion) => wrapper::enumeration::Enumeration::new(
                        expansion.enumeration(source)?,
                        expansion,
                    )
                    .render_exports(rust_type),
                }
            })
            .collect()
    }

    fn classes(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .classes
            .iter()
            .filter(|source| self.selected(source.id.as_str()))
            .map(|source| {
                let rust_type = self
                    .visible_paths
                    .get(source.id.as_str())
                    .map(Self::path_tokens)
                    .transpose()?;
                match (self.expansion, rust_type) {
                    (ExpansionSurface::Native(expansion), Some(rust_type)) => {
                        wrapper::class::Class::new(expansion.class(source)?, expansion)
                            .with_rust_type(rust_type)
                            .render()
                    }
                    (ExpansionSurface::Native(expansion), None) => {
                        wrapper::class::Class::new(expansion.class(source)?, expansion).render()
                    }
                    (ExpansionSurface::Wasm32(expansion), Some(rust_type)) => {
                        wrapper::class::Class::new(expansion.class(source)?, expansion)
                            .with_rust_type(rust_type)
                            .render()
                    }
                    (ExpansionSurface::Wasm32(expansion), None) => {
                        wrapper::class::Class::new(expansion.class(source)?, expansion).render()
                    }
                }
            })
            .collect()
    }

    fn streams(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .streams
            .iter()
            .filter(|source| self.selected(source.id.as_str()))
            .map(|source| {
                let owner = self.stream_owner(source)?;
                match (self.expansion, owner) {
                    (ExpansionSurface::Native(expansion), Some(owner)) => {
                        wrapper::stream::Stream::new(
                            expansion.stream(source)?,
                            expansion.class(owner.declaration)?,
                            owner.rust_type,
                            expansion,
                        )
                        .render()
                    }
                    (ExpansionSurface::Native(expansion), None) => {
                        wrapper::stream::Stream::function(expansion.stream(source)?, expansion)
                            .render()
                    }
                    (ExpansionSurface::Wasm32(expansion), Some(owner)) => {
                        wrapper::stream::Stream::new(
                            expansion.stream(source)?,
                            expansion.class(owner.declaration)?,
                            owner.rust_type,
                            expansion,
                        )
                        .render()
                    }
                    (ExpansionSurface::Wasm32(expansion), None) => {
                        wrapper::stream::Stream::function(expansion.stream(source)?, expansion)
                            .render()
                    }
                }
            })
            .collect()
    }

    fn constants(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .constants
            .iter()
            .filter(|source| {
                self.selected(source.id.as_str())
                    || source.owner.as_ref().is_some_and(|owner| {
                        self.selected.is_some() && self.selected(owner_id(owner))
                    })
            })
            .map(|source| {
                let owner = self.constant_owner(source)?;
                match (self.expansion, owner) {
                    (ExpansionSurface::Native(expansion), Some((owner, rust_type))) => {
                        wrapper::constant::Constant::new(expansion.constant(source)?, expansion)
                            .with_owner(owner, rust_type)
                            .render()
                    }
                    (ExpansionSurface::Native(expansion), None) => {
                        wrapper::constant::Constant::new(expansion.constant(source)?, expansion)
                            .render()
                    }
                    (ExpansionSurface::Wasm32(expansion), Some((owner, rust_type))) => {
                        wrapper::constant::Constant::new(expansion.constant(source)?, expansion)
                            .with_owner(owner, rust_type)
                            .render()
                    }
                    (ExpansionSurface::Wasm32(expansion), None) => {
                        wrapper::constant::Constant::new(expansion.constant(source)?, expansion)
                            .render()
                    }
                }
            })
            .collect()
    }

    fn functions(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .functions
            .iter()
            .filter(|source| self.selected(source.id.as_str()))
            .map(|source| {
                let path = self.visible_paths.get(source.id.as_str());
                match (self.expansion, path) {
                    (ExpansionSurface::Native(expansion), Some(path)) => {
                        wrapper::function::Function::new(expansion.function(source)?, expansion)
                            .with_path(path)?
                            .render()
                    }
                    (ExpansionSurface::Native(expansion), None) => {
                        wrapper::function::Function::new(expansion.function(source)?, expansion)
                            .render()
                    }
                    (ExpansionSurface::Wasm32(expansion), Some(path)) => {
                        wrapper::function::Function::new(expansion.function(source)?, expansion)
                            .with_path(path)?
                            .render()
                    }
                    (ExpansionSurface::Wasm32(expansion), None) => {
                        wrapper::function::Function::new(expansion.function(source)?, expansion)
                            .render()
                    }
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path as FsPath, PathBuf};
    use std::process::Command;

    use boltffi_ast::{
        CanonicalName, ClassDef, ConstExpr, ConstantDef, ConstantId, ConstantOwner, EnumDef,
        EnumId, FieldDef, FunctionDef, FunctionId, Literal, MethodDef, MethodId, PackageInfo,
        ParameterDef, Primitive, Receiver, RecordDef, ReprAttr, ReprItem, ReturnDef,
        SourceContract, SourceName, StreamDef, StreamId, TraitDef, TraitId, TypeExpr, VariantDef,
    };
    use boltffi_binding::{Native, Wasm32, lower_with_declarations};
    use proc_macro2::TokenStream;
    use quote::quote;

    use crate::expansion::{contract::Expansion, expander};

    #[test]
    fn expands_all_declaration_families_in_contract_order() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);
        let expander = expander::Expander::new(&source);

        let exports = expander.native(&expansion).expect("contract expands");
        let record_runtimes = source
            .records
            .iter()
            .map(|record| expander.record_runtime(&record.id, &expansion))
            .collect::<Result<Vec<_>, _>>()
            .expect("record runtimes expand");
        let enumeration_runtimes = source
            .enums
            .iter()
            .map(|enumeration| expander.enumeration_runtime(&enumeration.id, &expansion))
            .collect::<Result<Vec<_>, _>>()
            .expect("enum runtimes expand");
        let rendered = exports.to_string();

        assert_in_order(
            &rendered,
            &[
                "boltffi_register_callback_demo_listener",
                "fn boltffi_release_class_demo_engine",
                "fn boltffi_stream_demo_engine_values_subscribe",
                "fn boltffi_const_demo_magic",
                "fn boltffi_function_demo_answer",
            ],
        );
        let runtimes = quote! {
            #(#record_runtimes)*
            #(#enumeration_runtimes)*
        };
        let rendered_runtimes = runtimes.to_string();
        assert!(rendered_runtimes.contains("Passable for Point"));
        assert!(rendered_runtimes.contains("Passable for Status"));
        let tokens = quote! {
            #runtimes
            #exports
        };
        assert_generated_crate_checks("expander_all_declarations", full_contract_crate(tokens));
    }

    #[test]
    fn renders_ownerless_stream_from_free_function() {
        let source = ownerless_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);

        let tokens = expander::Expander::new(&source)
            .native(&expansion)
            .expect("contract expands");
        let rendered = tokens.to_string();

        assert!(rendered.contains("fn boltffi_stream_demo_events_subscribe () -> u64"));
        assert!(rendered.contains("let subscription = events ()"));
        assert_generated_crate_checks("expander_ownerless_stream", ownerless_stream_crate(tokens));
    }

    /// A hyphenated package scans and expands as one chain.
    ///
    /// `root_type` decides whether a declaration is local by comparing the id's
    /// first segment against the package name with hyphens replaced, so the
    /// scan has to write the id with the module name rather than the package
    /// name. Constructing the contract by hand proves nothing here: the two
    /// halves have to meet. Every fixture is named `demo`, which has no second
    /// spelling to disagree about.
    #[test]
    fn scans_and_expands_a_hyphenated_package() {
        let source = boltffi_scan::scan_file(
            syn::parse_str("#[data]\n#[repr(C)]\npub struct Point { pub x: u8 }\n")
                .expect("valid source"),
            PackageInfo::new("my-root", None),
        )
        .expect("source scans");
        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);

        expander::Expander::new(&source)
            .native(&expansion)
            .expect("a declaration of this crate expands");
    }

    #[test]
    fn renders_associated_constant_access_through_its_owner() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        let mut color = RecordDef::new("demo::Color".into(), CanonicalName::single("Color"));
        color.repr = ReprAttr::new(vec![ReprItem::C]);
        color.fields = ["r", "g", "b", "a"]
            .into_iter()
            .map(|field| {
                FieldDef::new(
                    CanonicalName::single(field),
                    TypeExpr::Primitive(Primitive::U8),
                )
            })
            .collect();
        source.records.push(color);
        let mut black = ConstantDef::new(
            ConstantId::new("demo::Color::BLACK"),
            SourceName::new("BLACK", CanonicalName::single("BLACK")),
            TypeExpr::SelfType,
            ConstExpr::Raw("Self::rgba(0, 0, 0, 255)".to_owned()),
        );
        black.owner = Some(ConstantOwner::Record("demo::Color".into()));
        source.constants.push(black);
        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);
        let expander = expander::Expander::new(&source);

        let exports = expander.native(&expansion).expect("contract expands");
        let runtime = expander
            .record_runtime(&source.records[0].id, &expansion)
            .expect("record runtime expands");
        let rendered = exports.to_string();
        let tokens = quote! {
            #runtime
            #exports
        };

        assert!(rendered.contains("Color :: BLACK"));
        assert_generated_crate_checks(
            "expander_associated_constant",
            quote! {
                #![allow(dead_code)]

                #[derive(Clone, Copy)]
                #[repr(C)]
                pub struct Color {
                    pub r: u8,
                    pub g: u8,
                    pub b: u8,
                    pub a: u8,
                }

                impl Color {
                    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
                }

                #tokens
            },
        );
    }

    #[test]
    fn unmarked_associated_constants_do_not_reach_macro_expansion() {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                pub struct Engine;

                #[export]
                impl Engine {
                    pub fn new() -> Self {
                        Self
                    }
                }

                impl Engine {
                    pub const UNEXPORTED_ASSOCIATED: u8 = 99;
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        assert!(source.constants.is_empty());

        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);
        let tokens = expander::Expander::new(&source)
            .native(&expansion)
            .expect("contract expands");

        assert!(!tokens.to_string().contains("unexported_associated"));
    }

    #[test]
    fn expands_native_and_wasm_surfaces_together() {
        let source = source_contract();
        let native_lowered = lower_with_declarations::<Native>(&source).expect("native lowers");
        let wasm32_lowered = lower_with_declarations::<Wasm32>(&source).expect("wasm lowers");
        let native = Expansion::new(&native_lowered);
        let wasm32 = Expansion::new(&wasm32_lowered);

        let tokens = expander::Expander::new(&source)
            .all(&native, &wasm32)
            .expect("all surfaces expand");
        let rendered = tokens.to_string();

        assert!(rendered.contains("# [cfg (not (target_arch = \"wasm32\"))]"));
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("mod __boltffi_native"));
        assert!(rendered.contains("mod __boltffi_wasm32"));
        assert_generated_crate_checks("expander_all_surfaces", full_contract_crate(tokens));
    }

    fn source_contract() -> SourceContract {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.records.push(point_record());
        source.enums.push(status_enum());
        source.classes.push(engine_class());
        source.streams.push(stream());
        source.constants.push(bytes_constant());
        source.functions.push(answer_function());
        source
    }

    fn ownerless_stream_contract() -> SourceContract {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.streams.push(StreamDef::new(
            StreamId::new("demo::events"),
            CanonicalName::single("events"),
            TypeExpr::Primitive(Primitive::U32),
        ));
        source
    }

    fn listener_trait() -> TraitDef {
        let mut listener = TraitDef::new(
            TraitId::new("demo::Listener"),
            CanonicalName::single("Listener"),
        );
        let mut method = MethodDef::new(
            MethodId::new("on_value"),
            CanonicalName::single("on_value"),
            Receiver::Shared,
        );
        method.parameters = vec![ParameterDef::value(
            CanonicalName::single("value"),
            TypeExpr::Primitive(Primitive::U32),
        )];
        method.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));
        listener.methods.push(method);
        listener
    }

    fn point_record() -> RecordDef {
        let mut record = RecordDef::new("demo::Point".into(), CanonicalName::single("Point"));
        record.repr = ReprAttr::new(vec![ReprItem::C]);
        record.fields = vec![FieldDef::new(
            CanonicalName::single("x"),
            TypeExpr::Primitive(Primitive::F64),
        )];
        record
    }

    fn status_enum() -> EnumDef {
        let mut enumeration =
            EnumDef::new(EnumId::new("demo::Status"), CanonicalName::single("Status"));
        enumeration.variants = vec![
            VariantDef::unit(SourceName::new("Ready", CanonicalName::single("Ready"))),
            VariantDef::unit(SourceName::new("Done", CanonicalName::single("Done"))),
        ];
        enumeration
    }

    fn engine_class() -> ClassDef {
        ClassDef::new("demo::Engine".into(), CanonicalName::single("Engine"))
    }

    fn stream() -> StreamDef {
        let mut stream = StreamDef::new(
            StreamId::new("demo::Engine::values"),
            CanonicalName::single("values"),
            TypeExpr::Primitive(Primitive::U32),
        );
        stream.owner = Some("demo::Engine".into());
        stream
    }

    fn bytes_constant() -> ConstantDef {
        ConstantDef::new(
            ConstantId::new("demo::MAGIC"),
            SourceName::new("MAGIC", CanonicalName::single("MAGIC")),
            TypeExpr::slice(TypeExpr::Primitive(Primitive::U8)),
            ConstExpr::Literal(Literal::Bytes(b"ffi".to_vec())),
        )
    }

    fn answer_function() -> FunctionDef {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));
        function
    }

    fn assert_in_order(source: &str, needles: &[&str]) {
        needles
            .iter()
            .try_fold(0usize, |offset, needle| {
                source[offset..]
                    .find(needle)
                    .map(|position| offset + position + needle.len())
                    .ok_or(needle)
            })
            .expect("rendered contract preserves declaration order");
    }

    fn full_contract_crate(tokens: TokenStream) -> TokenStream {
        quote! {
            #![allow(dead_code)]

            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub trait Listener {
                fn on_value(&self, value: u32) -> u32;
            }

            #[derive(Clone, Copy)]
            #[repr(C)]
            pub struct Point {
                pub x: f64,
            }

            pub enum Status {
                Ready,
                Done,
            }

            pub struct Engine {
                producer: StreamProducer<u32>,
            }

            impl Engine {
                pub fn values(&self) -> Arc<EventSubscription<u32>> {
                    self.producer.subscribe()
                }
            }

            pub const MAGIC: &[u8] = b"ffi";

            pub fn answer() -> u32 {
                42
            }

            #tokens
        }
    }

    fn ownerless_stream_crate(tokens: TokenStream) -> TokenStream {
        quote! {
            #![allow(dead_code)]

            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub fn events() -> Arc<EventSubscription<u32>> {
                StreamProducer::<u32>::new(16).subscribe()
            }

            #tokens
        }
    }

    fn assert_generated_crate_checks(name: &str, code: TokenStream) {
        let generated_crate = GeneratedCrate::create(name);
        generated_crate.write(crate::capture::with_trait_identities(code));
        generated_crate.check();
    }

    struct GeneratedCrate {
        root: PathBuf,
        output: GeneratedCrateOutput,
    }

    impl GeneratedCrate {
        fn create(name: &str) -> Self {
            Self::new(name, GeneratedCrateOutput::Library)
        }

        fn new(name: &str, output: GeneratedCrateOutput) -> Self {
            if cfg!(miri) {
                return Self {
                    root: PathBuf::new(),
                    output,
                };
            }
            let root = workspace_root()
                .join("target")
                .join("expander-checks")
                .join(format!("{}-{}", name, std::process::id()));
            if root.exists() {
                fs::remove_dir_all(&root).expect("remove old generated crate");
            }
            fs::create_dir_all(root.join("src")).expect("create generated crate");
            Self { root, output }
        }

        fn write(&self, code: TokenStream) {
            if cfg!(miri) {
                return;
            }
            fs::write(self.root.join("Cargo.toml"), self.manifest()).expect("write Cargo.toml");
            fs::write(self.root.join("src/lib.rs"), code.to_string()).expect("write lib.rs");
        }

        fn check(&self) {
            if cfg!(miri) {
                return;
            }
            let output = Command::new(cargo())
                .arg("check")
                .arg("--quiet")
                .arg("--manifest-path")
                .arg(self.root.join("Cargo.toml"))
                .env(
                    "CARGO_TARGET_DIR",
                    workspace_root()
                        .join("target")
                        .join("expander-checks-target"),
                )
                .output()
                .expect("run cargo check for generated crate");
            assert!(
                output.status.success(),
                "generated crate failed to check\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn manifest(&self) -> String {
            let crate_type = self.output.manifest_section();
            format!(
                "[package]\nname = \"generated_expander_check\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n{crate_type}\n[dependencies]\nboltffi = {{ path = \"{}\" }}\n",
                workspace_root().join("boltffi").display()
            )
        }
    }

    #[derive(Clone, Copy)]
    enum GeneratedCrateOutput {
        Library,
    }

    impl GeneratedCrateOutput {
        const fn manifest_section(self) -> &'static str {
            match self {
                Self::Library => "\n",
            }
        }
    }

    fn workspace_root() -> PathBuf {
        FsPath::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf()
    }

    fn cargo() -> OsString {
        std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
    }
}
