use std::collections::HashMap;

use boltffi_ast::{ClassDef, Path, PathRoot, SourceContract, StreamDef, TraitDef};
use boltffi_binding::{Native, SerializedBindings, Wasm32};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    metadata,
    surface::RenderSurface,
    wrapper::{self, Render},
};

/// A crate-level wrapper expander.
///
/// The expander owns target selection for an already scanned source contract.
/// Each method accepts the matching lowered expansion, so native and wasm
/// bindings cannot be crossed accidentally.
pub struct Expander<'lowered> {
    source: &'lowered SourceContract,
    support: &'lowered SourceContract,
    visible_paths: HashMap<String, Path>,
}

struct SurfaceExpander<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered SourceContract,
    support: &'lowered SourceContract,
    visible_paths: &'expansion HashMap<String, Path>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'lowered> Expander<'lowered> {
    /// Creates an expander over the scanned source contract.
    pub fn new(source: &'lowered SourceContract) -> Self {
        Self {
            source,
            support: source,
            visible_paths: HashMap::new(),
        }
    }

    pub fn with_support(
        source: &'lowered SourceContract,
        support: &'lowered SourceContract,
        visible_paths: impl IntoIterator<Item = (String, Path)>,
    ) -> Self {
        Self {
            source,
            support,
            visible_paths: visible_paths.into_iter().collect(),
        }
    }

    /// Expands wrappers for the native surface.
    pub fn native(&self, expansion: &Expansion<'lowered, Native>) -> Result<TokenStream, Error> {
        let wrappers =
            SurfaceExpander::new(self.source, self.support, &self.visible_paths, expansion)
                .expand()?;
        let metadata = metadata::render(SerializedBindings::native(expansion.bindings().clone()))?;

        Ok(quote! {
            #wrappers
            #metadata
        })
    }

    /// Expands wrappers for the wasm32 surface.
    pub fn wasm32(&self, expansion: &Expansion<'lowered, Wasm32>) -> Result<TokenStream, Error> {
        let wrappers =
            SurfaceExpander::new(self.source, self.support, &self.visible_paths, expansion)
                .expand()?;
        let metadata = metadata::render(SerializedBindings::wasm32(expansion.bindings().clone()))?;

        Ok(quote! {
            #wrappers
            #metadata
        })
    }

    /// Expands wrappers for native and wasm32 in one token stream.
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

impl<'expansion, 'lowered, S> SurfaceExpander<'expansion, 'lowered, S>
where
    S: RenderSurface,
    wrapper::callback::Renderer:
        Render<S, wrapper::callback::Trait<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::handle::Carrier: Render<
            S,
            wrapper::handle::CarrierInput<S::HandleCarrier>,
            Output = wrapper::handle::CarrierTokens,
        >,
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    wrapper::param::direct::Renderer:
        Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
    wrapper::returns::encoded::Renderer:
        Render<S, wrapper::returns::encoded::Empty<S>, Output = wrapper::returns::encoded::Tokens>,
    for<'codec> wrapper::returns::encoded::Renderer: Render<
            S,
            wrapper::returns::encoded::Input<'expansion, 'codec, 'lowered, S>,
            Output = wrapper::returns::encoded::Tokens,
        >,
{
    const fn new(
        source: &'lowered SourceContract,
        support: &'lowered SourceContract,
        visible_paths: &'expansion HashMap<String, Path>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            source,
            support,
            visible_paths,
            expansion,
        }
    }

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

    /// Explicit imports for the root crate's record and enum impl targets.
    ///
    /// The wrapper module only glob-imports the crate root, while codec
    /// impls spell their target by bare leaf name, so a root type that is
    /// not re-exported at the root would not resolve. Explicit imports
    /// shadow the glob, which keeps root re-exports working unchanged.
    /// Dependency types keep their own crate's codec impls and need no
    /// import here.
    fn record_and_enum_imports(&self) -> Result<Vec<TokenStream>, Error> {
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

    fn callbacks(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .traits
            .iter()
            .map(|source| {
                let callback = wrapper::callback::Trait::new(
                    self.expansion.callback_trait(source)?,
                    self.expansion,
                )
                .with_path(self.trait_path(source)?, self.trait_object_impls(source));
                <wrapper::callback::Renderer as Render<S, _>>::render(
                    wrapper::callback::Renderer,
                    callback,
                )
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

    fn records(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .records
            .iter()
            .map(|source| {
                wrapper::record::Renderer::new(self.expansion.record(source)?, self.expansion)
                    .render()
            })
            .collect()
    }

    fn enumerations(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .enums
            .iter()
            .map(|source| {
                wrapper::enumeration::Renderer::new(
                    self.expansion.enumeration(source)?,
                    self.expansion,
                )
                .render()
            })
            .collect()
    }

    fn classes(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .classes
            .iter()
            .map(|source| {
                let renderer =
                    wrapper::class::Renderer::new(self.expansion.class(source)?, self.expansion);
                match self.visible_paths.get(source.id.as_str()) {
                    Some(path) => renderer.with_rust_type(Self::path_tokens(path)?).render(),
                    None => renderer.render(),
                }
            })
            .collect()
    }

    fn streams(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .streams
            .iter()
            .map(|source| {
                let owner = self.stream_owner(source)?;
                let stream = self.expansion.stream(source)?;
                match owner {
                    Some(owner) => wrapper::stream::Renderer::new(
                        stream,
                        self.expansion.class(owner)?,
                        self.expansion,
                    )
                    .render(),
                    None => wrapper::stream::Renderer::function(stream, self.expansion).render(),
                }
            })
            .collect()
    }

    fn constants(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .constants
            .iter()
            .map(|source| {
                wrapper::constant::Renderer::new(self.expansion.constant(source)?, self.expansion)
                    .render()
            })
            .collect()
    }

    fn functions(&self) -> Result<Vec<TokenStream>, Error> {
        self.support
            .functions
            .iter()
            .map(|source| {
                let renderer = wrapper::function::Renderer::new(
                    self.expansion.function(source)?,
                    self.expansion,
                );
                match self.visible_paths.get(source.id.as_str()) {
                    Some(path) => renderer.with_path(path)?.render(),
                    None => renderer.render(),
                }
            })
            .collect()
    }

    fn stream_owner(&self, stream: &StreamDef) -> Result<Option<&'lowered ClassDef>, Error> {
        stream
            .owner
            .as_ref()
            .map(|owner| {
                self.support
                    .classes
                    .iter()
                    .find(|class| &class.id == owner)
                    .ok_or(Error::SourceSyntaxMismatch("stream owner class is missing"))
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path as FsPath, PathBuf};
    use std::process::Command;

    use boltffi_ast::{
        CanonicalName, ClassDef, ConstExpr, ConstantDef, ConstantId, EnumDef, EnumId, FieldDef,
        FunctionDef, FunctionId, Literal, MethodDef, MethodId, PackageInfo, ParameterDef,
        Primitive, Receiver, RecordDef, ReprAttr, ReprItem, ReturnDef, SourceContract, SourceName,
        StreamDef, StreamId, TraitDef, TraitId, TypeExpr, VariantDef,
    };
    use boltffi_bindgen::artifact::BindingMetadataReader;
    use boltffi_binding::{
        BindingMetadataSection, BindingMetadataSurface, Native, SerializedBindings, Wasm32,
        lower_with_declarations,
    };
    use proc_macro2::TokenStream;
    use quote::quote;

    use crate::experimental::{expander, expansion::Expansion};

    #[test]
    fn expands_all_declaration_families_in_contract_order() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);

        let tokens = expander::Expander::new(&source)
            .native(&expansion)
            .expect("contract expands");
        let rendered = tokens.to_string();

        assert_in_order(
            &rendered,
            &[
                "boltffi_register_callback_demo_listener",
                "unsafe impl :: boltffi :: __private :: Passable for Point",
                "unsafe impl :: boltffi :: __private :: Passable for Status",
                "fn boltffi_release_class_demo_engine",
                "fn boltffi_stream_demo_engine_values_subscribe",
                "fn boltffi_const_demo_magic",
                "fn boltffi_function_demo_answer",
            ],
        );
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

    #[test]
    fn native_expander_imports_root_codec_targets_by_visible_path() {
        // The record impls spell their target by bare leaf name inside the
        // wrapper module, so a root type living outside the crate root needs
        // an explicit import along its public re-export.
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        let mut record = RecordDef::new(
            "demo::inner::Hidden".into(),
            CanonicalName::single("Hidden"),
        );
        record.fields = vec![FieldDef::new(
            CanonicalName::single("x"),
            TypeExpr::Primitive(Primitive::F64),
        )];
        source.records.push(record);
        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);
        let visible_paths = vec![(
            "demo::inner::Hidden".to_owned(),
            boltffi_ast::Path::new(
                boltffi_ast::PathRoot::Crate,
                vec![boltffi_ast::PathSegment::new("Hidden")],
            ),
        )];

        let tokens = expander::Expander::with_support(&source, &source, visible_paths)
            .native(&expansion)
            .expect("contract expands");
        let rendered = tokens.to_string();

        assert!(rendered.contains("use crate :: Hidden ;"));
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

    #[test]
    fn native_expander_emits_binding_metadata_static() {
        let source = ownerless_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("native lowers");
        let expansion = Expansion::new(&lowered);

        let tokens = expander::Expander::new(&source)
            .native(&expansion)
            .expect("native expands");
        let rendered = tokens.to_string();

        assert!(rendered.contains("boltffi_metadata"));
        assert!(rendered.contains("not (target_arch = \"wasm32\")"));
        assert!(rendered.contains(&format!(
            "unsafe (link_section = {:?})",
            BindingMetadataSection::MachO.link_section()
        )));
        assert!(rendered.contains(&format!(
            "unsafe (link_section = {:?})",
            BindingMetadataSection::Object.link_section()
        )));
        assert!(rendered.contains("# [used]"));
        assert!(rendered.contains("const _ : ()"));
        assert!(rendered.contains("static __BOLTFFI_BINDINGS"));
    }

    #[test]
    fn wasm32_expander_emits_binding_metadata_static() {
        let source = ownerless_stream_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("wasm lowers");
        let expansion = Expansion::new(&lowered);

        let tokens = expander::Expander::new(&source)
            .wasm32(&expansion)
            .expect("wasm expands");
        let rendered = tokens.to_string();

        assert!(rendered.contains("boltffi_metadata"));
        assert!(rendered.contains("target_arch = \"wasm32\""));
        assert!(rendered.contains(&format!(
            "unsafe (link_section = {:?})",
            BindingMetadataSection::MachO.link_section()
        )));
        assert!(rendered.contains(&format!(
            "unsafe (link_section = {:?})",
            BindingMetadataSection::Object.link_section()
        )));
        assert!(rendered.contains("# [used]"));
        assert!(rendered.contains("const _ : ()"));
        assert!(rendered.contains("static __BOLTFFI_BINDINGS"));
    }

    #[test]
    fn repeated_metadata_emission_checks_with_metadata_cfg_enabled() {
        let source = ownerless_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("native lowers");
        let metadata =
            super::metadata::render(SerializedBindings::native(lowered.bindings().clone()))
                .expect("metadata renders");

        assert_generated_crate_checks_with_rustflags(
            "expander_repeated_metadata",
            quote! {
                #![deny(warnings)]

                #metadata
                #metadata
            },
            "--cfg boltffi_metadata",
        );
    }

    #[test]
    fn native_expander_metadata_is_read_from_compiled_artifact() {
        if cfg!(miri) {
            return;
        }
        let source = ownerless_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("native lowers");
        let expected_bindings = SerializedBindings::native(lowered.bindings().clone());
        let expansion = Expansion::new(&lowered);
        let tokens = expander::Expander::new(&source)
            .native(&expansion)
            .expect("native expands");
        let generated_crate = GeneratedCrate::static_library("expander_metadata_artifact");
        generated_crate.write(ownerless_stream_crate(tokens));

        let artifact = generated_crate.build_staticlib_with_rustflags("--cfg boltffi_metadata");
        let envelopes = BindingMetadataReader::new([artifact])
            .read_required()
            .expect("compiled metadata reads");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].surface(), BindingMetadataSurface::Native);
        assert_eq!(envelopes[0].package().name().as_path_string(), "demo");
        assert_eq!(envelopes[0].bindings(), &expected_bindings);
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
        generated_crate.write(code);
        generated_crate.check();
    }

    fn assert_generated_crate_checks_with_rustflags(
        name: &str,
        code: TokenStream,
        rustflags: &str,
    ) {
        let generated_crate = GeneratedCrate::create(name);
        generated_crate.write(code);
        generated_crate.check_with_rustflags(rustflags);
    }

    struct GeneratedCrate {
        root: PathBuf,
        output: GeneratedCrateOutput,
    }

    impl GeneratedCrate {
        fn create(name: &str) -> Self {
            Self::new(name, GeneratedCrateOutput::Library)
        }

        fn static_library(name: &str) -> Self {
            Self::new(name, GeneratedCrateOutput::StaticLibrary)
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
                .join("experimental-expander-checks")
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
                        .join("experimental-expander-checks-target"),
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

        fn check_with_rustflags(&self, rustflags: &str) {
            if cfg!(miri) {
                return;
            }
            let output = Command::new(cargo())
                .arg("check")
                .arg("--quiet")
                .arg("--manifest-path")
                .arg(self.root.join("Cargo.toml"))
                .env("RUSTFLAGS", rustflags)
                .env(
                    "CARGO_TARGET_DIR",
                    workspace_root()
                        .join("target")
                        .join("experimental-expander-checks-target"),
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

        fn build_staticlib_with_rustflags(&self, rustflags: &str) -> PathBuf {
            if cfg!(miri) {
                return PathBuf::new();
            }
            let target = workspace_root()
                .join("target")
                .join("experimental-expander-artifacts");
            let output = Command::new(cargo())
                .arg("build")
                .arg("--quiet")
                .arg("--release")
                .arg("--manifest-path")
                .arg(self.root.join("Cargo.toml"))
                .env("RUSTFLAGS", rustflags)
                .env("CARGO_TARGET_DIR", &target)
                .output()
                .expect("run cargo build for generated staticlib");
            assert!(
                output.status.success(),
                "generated staticlib failed to build\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            self.staticlib_artifact(&target)
        }

        fn staticlib_artifact(&self, target: &FsPath) -> PathBuf {
            let release = target.join("release");
            let extension = if cfg!(target_os = "windows") {
                "lib"
            } else {
                "a"
            };
            fs::read_dir(&release)
                .expect("read generated staticlib output directory")
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| {
                    path.extension().is_some_and(|actual| actual == extension)
                        && path
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .is_some_and(|stem| stem.contains("generated_expander_check"))
                })
                .expect("generated staticlib artifact exists")
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
        StaticLibrary,
    }

    impl GeneratedCrateOutput {
        const fn manifest_section(self) -> &'static str {
            match self {
                Self::Library => "\n",
                Self::StaticLibrary => "\n[lib]\ncrate-type = [\"staticlib\"]\n",
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
