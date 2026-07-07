mod index;
mod pair;

use boltffi_ast::{ClassDef, ConstantDef, EnumDef, FunctionDef, RecordDef, StreamDef, TraitDef};
use boltffi_binding::{
    Bindings, CallbackDecl, CallbackId, ClassDecl, ConstantDecl, CustomTypeDecl, CustomTypeId,
    EncodedRecordDecl, EnumDecl, FunctionDecl, LoweredBindings, RecordDecl, RecordId, StreamDecl,
    Surface,
};

use self::index::ExpansionIndex;
use self::pair::{PairedDeclaration, SourceDeclaration};
use super::error::Error;

pub use self::pair::DeclarationPair;

/// An indexed lowered crate for one target surface.
///
/// The value pairs scanned source declarations with their lowered binding declarations.
/// It does not render Rust syntax, choose target sets, scan source, or run lowering.
pub struct Expansion<'lowered, S: Surface> {
    lowered: &'lowered LoweredBindings<S>,
    index: ExpansionIndex,
}

impl<'lowered, S: Surface> Expansion<'lowered, S> {
    /// Creates an indexed view over lowered bindings for one target surface.
    pub fn new(lowered: &'lowered LoweredBindings<S>) -> Self {
        Self {
            lowered,
            index: ExpansionIndex::new(lowered),
        }
    }

    /// Returns the lowered binding declarations.
    pub fn bindings(&self) -> &'lowered Bindings<S> {
        self.lowered.bindings()
    }

    /// Returns the custom declaration for a custom codec node.
    pub fn custom_type(&self, id: CustomTypeId) -> Result<&'lowered CustomTypeDecl, Error> {
        self.index.custom_type(self.lowered, id)
    }

    /// Returns the callback declaration for a callback handle target.
    pub fn callback(&self, id: CallbackId) -> Result<&'lowered CallbackDecl<S>, Error> {
        self.index.callback(self.lowered, id)
    }

    /// Returns the lowered callback declaration paired with the scanned source trait.
    pub fn callback_trait(
        &self,
        source: &'lowered TraitDef,
    ) -> Result<DeclarationPair<'lowered, TraitDef, CallbackDecl<S>>, Error> {
        match self
            .index
            .paired(self.lowered, SourceDeclaration::Callback(source))?
        {
            PairedDeclaration::Callback(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }

    /// Returns the encoded record declaration for an encoded record codec node.
    pub fn encoded_record(&self, id: RecordId) -> Result<&'lowered EncodedRecordDecl<S>, Error> {
        self.index.encoded_record(self.lowered, id)
    }

    /// Returns the lowered record declaration paired with the scanned source record.
    pub fn record(
        &self,
        source: &'lowered RecordDef,
    ) -> Result<DeclarationPair<'lowered, RecordDef, RecordDecl<S>>, Error> {
        match self
            .index
            .paired(self.lowered, SourceDeclaration::Record(source))?
        {
            PairedDeclaration::Record(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }

    /// Returns the lowered enum declaration paired with the scanned source enum.
    pub fn enumeration(
        &self,
        source: &'lowered EnumDef,
    ) -> Result<DeclarationPair<'lowered, EnumDef, EnumDecl<S>>, Error> {
        match self
            .index
            .paired(self.lowered, SourceDeclaration::Enum(source))?
        {
            PairedDeclaration::Enum(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }

    /// Returns the lowered class declaration paired with the scanned source class.
    pub fn class(
        &self,
        source: &'lowered ClassDef,
    ) -> Result<DeclarationPair<'lowered, ClassDef, ClassDecl<S>>, Error> {
        match self
            .index
            .paired(self.lowered, SourceDeclaration::Class(source))?
        {
            PairedDeclaration::Class(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }

    /// Returns the lowered stream declaration paired with the scanned source stream.
    pub fn stream(
        &self,
        source: &'lowered StreamDef,
    ) -> Result<DeclarationPair<'lowered, StreamDef, StreamDecl<S>>, Error> {
        match self
            .index
            .paired(self.lowered, SourceDeclaration::Stream(source))?
        {
            PairedDeclaration::Stream(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }

    /// Returns the lowered constant declaration paired with the scanned source constant.
    pub fn constant(
        &self,
        source: &'lowered ConstantDef,
    ) -> Result<DeclarationPair<'lowered, ConstantDef, ConstantDecl<S>>, Error> {
        match self
            .index
            .paired(self.lowered, SourceDeclaration::Constant(source))?
        {
            PairedDeclaration::Constant(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }

    /// Returns the lowered function declaration paired with the scanned source function.
    pub fn function(
        &self,
        source: &'lowered FunctionDef,
    ) -> Result<DeclarationPair<'lowered, FunctionDef, FunctionDecl<S>>, Error> {
        match self
            .index
            .paired(self.lowered, SourceDeclaration::Function(source))?
        {
            PairedDeclaration::Function(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path as FsPath, PathBuf};
    use std::process::Command;

    use boltffi_ast::{
        AdditionalBound, AttributeInput, BaseTrait, CanonicalName, ClassDef, ClassId,
        ClassThreadSafety, ConstExpr, ConstantDef, ConstantId, CustomRemoteType,
        CustomTypeConverter, CustomTypeConverters, CustomTypeDef, CustomTypeId, EnumDef, EnumId,
        ExecutionKind, FieldDef, FnSig, FnTrait, FnTraitKind, FunctionDef, FunctionId,
        IntegerLiteral, Literal, MethodDef, MethodId, PackageInfo, ParameterDef, ParameterPassing,
        Path, Primitive, Receiver, RecordDef, RecordId, ReprAttr, ReprItem, ReturnDef, Source,
        SourceContract, SourceName, StreamDef, StreamId, TraitBounds, TraitDef, TraitId, TypeExpr,
        UserAttr, VariantDef, VariantPayload, Visibility,
    };
    use boltffi_binding::{Native, Wasm32, lower_with_declarations};
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::ItemFn;

    use super::Expansion;
    use crate::experimental::surface::RenderSurface;
    use crate::experimental::{error::Error, wrapper};

    struct GeneratedCrate {
        root: PathBuf,
    }

    impl GeneratedCrate {
        fn create(name: &str) -> Self {
            if cfg!(miri) {
                return Self {
                    root: PathBuf::new(),
                };
            }
            let root = workspace_root()
                .join("target")
                .join("experimental-wrapper-checks")
                .join(format!("{}-{}", name, std::process::id()));
            if root.exists() {
                fs::remove_dir_all(&root).expect("remove old generated crate");
            }
            fs::create_dir_all(root.join("src")).expect("create generated crate");
            Self { root }
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
                        .join("experimental-wrapper-checks-target"),
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

        fn check_target(&self, target_triple: &str) {
            if cfg!(miri) {
                return;
            }
            assert!(
                target_installed(target_triple),
                "rust target {target_triple} is not installed"
            );
            let output = Command::new(cargo())
                .arg("check")
                .arg("--quiet")
                .arg("--target")
                .arg(target_triple)
                .arg("--manifest-path")
                .arg(self.root.join("Cargo.toml"))
                .env(
                    "CARGO_TARGET_DIR",
                    workspace_root()
                        .join("target")
                        .join("experimental-wrapper-checks-target"),
                )
                .output()
                .expect("run cargo target check for generated crate");
            assert!(
                output.status.success(),
                "generated crate failed to check for {target_triple}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn manifest(&self) -> String {
            format!(
                "[package]\nname = \"generated_wrapper_check\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\nboltffi = {{ path = \"{}\" }}\n",
                workspace_root().join("boltffi").display()
            )
        }
    }

    fn assert_generated_crate_checks(name: &str, code: TokenStream) {
        let generated_crate = GeneratedCrate::create(name);
        generated_crate.write(code);
        generated_crate.check();
    }

    fn assert_generated_crate_checks_target(name: &str, target_triple: &str, code: TokenStream) {
        let generated_crate = GeneratedCrate::create(name);
        generated_crate.write(code);
        generated_crate.check_target(target_triple);
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

    fn target_installed(target_triple: &str) -> bool {
        Command::new("rustup")
            .arg("target")
            .arg("list")
            .arg("--installed")
            .output()
            .ok()
            .is_some_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .any(|installed| installed == target_triple)
            })
    }

    fn expand_function<'lowered, S>(
        expansion: &Expansion<'lowered, S>,
        source: &'lowered FunctionDef,
        syntax: ItemFn,
    ) -> Result<TokenStream, Error>
    where
        S: RenderSurface,
        for<'expansion> wrapper::arguments::SyncRenderer: wrapper::Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        for<'expansion> wrapper::returns::Failure: wrapper::Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
        for<'expansion> wrapper::returns::Renderer: wrapper::Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        for<'expansion> wrapper::async_call::Renderer: wrapper::Render<
                S,
                wrapper::async_call::Input<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
    {
        let wrapper =
            wrapper::function::Renderer::new(expansion.function(source)?, expansion).render()?;

        Ok(quote! {
            #syntax
            #wrapper
        })
    }

    fn expand_native_callback<'lowered>(
        expansion: &Expansion<'lowered, Native>,
        source: &'lowered TraitDef,
    ) -> Result<TokenStream, Error> {
        let callback = wrapper::callback::Trait::new(expansion.callback_trait(source)?, expansion);
        <wrapper::callback::Renderer as wrapper::Render<Native, _>>::render(
            wrapper::callback::Renderer,
            callback,
        )
    }

    fn expand_wasm_callback<'lowered>(
        expansion: &Expansion<'lowered, Wasm32>,
        source: &'lowered TraitDef,
    ) -> Result<TokenStream, Error> {
        let callback = wrapper::callback::Trait::new(expansion.callback_trait(source)?, expansion);
        <wrapper::callback::Renderer as wrapper::Render<Wasm32, _>>::render(
            wrapper::callback::Renderer,
            callback,
        )
    }

    fn expand_record<'lowered, S>(
        expansion: &Expansion<'lowered, S>,
        source: &'lowered RecordDef,
    ) -> Result<TokenStream, Error>
    where
        S: RenderSurface,
        for<'expansion> wrapper::arguments::SyncRenderer: wrapper::Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        for<'expansion> wrapper::returns::Failure: wrapper::Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
        for<'expansion> wrapper::returns::Renderer: wrapper::Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        for<'expansion> wrapper::async_call::Renderer: wrapper::Render<
                S,
                wrapper::async_call::Input<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
        wrapper::param::direct::Record: wrapper::Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
        for<'expansion> wrapper::param::encoded::Renderer: wrapper::Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
    {
        wrapper::record::Renderer::new(expansion.record(source)?, expansion).render()
    }

    fn expand_enumeration<'lowered, S>(
        expansion: &Expansion<'lowered, S>,
        source: &'lowered EnumDef,
    ) -> Result<TokenStream, Error>
    where
        S: RenderSurface,
        for<'expansion> wrapper::arguments::SyncRenderer: wrapper::Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        for<'expansion> wrapper::returns::Failure: wrapper::Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
        for<'expansion> wrapper::returns::Renderer: wrapper::Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        for<'expansion> wrapper::async_call::Renderer: wrapper::Render<
                S,
                wrapper::async_call::Input<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
        wrapper::param::direct::Renderer:
            wrapper::Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
        for<'expansion> wrapper::param::encoded::Renderer: wrapper::Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
    {
        wrapper::enumeration::Renderer::new(expansion.enumeration(source)?, expansion).render()
    }

    fn expand_class<'lowered, S>(
        expansion: &Expansion<'lowered, S>,
        source: &'lowered ClassDef,
    ) -> Result<TokenStream, Error>
    where
        S: RenderSurface,
        wrapper::handle::Carrier: wrapper::Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
        for<'expansion> wrapper::arguments::SyncRenderer: wrapper::Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        for<'expansion> wrapper::returns::Failure: wrapper::Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
        for<'expansion> wrapper::returns::Renderer: wrapper::Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        for<'expansion> wrapper::async_call::Renderer: wrapper::Render<
                S,
                wrapper::async_call::Input<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
    {
        wrapper::class::Renderer::new(expansion.class(source)?, expansion).render()
    }

    fn expand_stream<'expansion, 'lowered, S>(
        expansion: &'expansion Expansion<'lowered, S>,
        stream: &'lowered StreamDef,
        owner: &'lowered ClassDef,
    ) -> Result<TokenStream, Error>
    where
        S: RenderSurface,
        wrapper::handle::Carrier: wrapper::Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
        wrapper::returns::encoded::Renderer: wrapper::Render<
                S,
                wrapper::returns::encoded::Empty<S>,
                Output = wrapper::returns::encoded::Tokens,
            >,
        wrapper::returns::encoded::Renderer: for<'codec> wrapper::Render<
                S,
                wrapper::returns::encoded::Input<'expansion, 'codec, 'lowered, S>,
                Output = wrapper::returns::encoded::Tokens,
            >,
    {
        wrapper::stream::Renderer::new(
            expansion.stream(stream)?,
            expansion.class(owner)?,
            expansion,
        )
        .render()
    }

    fn expand_constant<'lowered, S>(
        expansion: &Expansion<'lowered, S>,
        source: &'lowered ConstantDef,
    ) -> Result<TokenStream, Error>
    where
        S: RenderSurface,
        for<'expansion> wrapper::arguments::SyncRenderer: wrapper::Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        for<'expansion> wrapper::returns::Failure: wrapper::Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
        for<'expansion> wrapper::returns::Renderer: wrapper::Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        for<'expansion> wrapper::async_call::Renderer: wrapper::Render<
                S,
                wrapper::async_call::Input<'expansion, 'lowered, S>,
                Output = TokenStream,
            >,
    {
        wrapper::constant::Renderer::new(expansion.constant(source)?, expansion).render()
    }

    fn source_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn source_name_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::http_request"),
            SourceName::new("HTTPRequest", CanonicalName::single("http_request")),
        );
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn source_parameter_name_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::syntax_payload"),
            CanonicalName::single("syntax_payload"),
        );
        function.parameters = vec![ParameterDef::value(
            SourceName::new("HTTPCode", CanonicalName::single("http_code")),
            TypeExpr::Primitive(Primitive::U32),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn source_visibility_contract(visibility: Visibility) -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.source = Source::new(visibility, None);
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn constant_contract(
        id: &str,
        name: &str,
        type_expr: TypeExpr,
        value: ConstExpr,
    ) -> SourceContract {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.constants.push(ConstantDef::new(
            ConstantId::new(id),
            SourceName::new(name, CanonicalName::single(name)),
            type_expr,
            value,
        ));
        source
    }

    fn path(name: &str) -> Path {
        Path::single(name)
    }

    fn record(name: &str) -> TypeExpr {
        TypeExpr::record(RecordId::new(format!("demo::{name}")), path(name))
    }

    fn enumeration(name: &str) -> TypeExpr {
        TypeExpr::enumeration(EnumId::new(format!("demo::{name}")), path(name))
    }

    fn class(name: &str) -> TypeExpr {
        TypeExpr::class(ClassId::new(format!("demo::{name}")), path(name))
    }

    fn custom_timestamp() -> TypeExpr {
        TypeExpr::custom(CustomTypeId::new("demo::Timestamp"), path("Timestamp"))
    }

    fn timestamp_custom_def() -> CustomTypeDef {
        CustomTypeDef::new(
            CustomTypeId::new("demo::Timestamp"),
            CanonicalName::single("Timestamp"),
            CustomRemoteType::single_path("Timestamp"),
            TypeExpr::Primitive(Primitive::I64),
            None,
            CustomTypeConverters::new(
                CustomTypeConverter::path(Path::single("timestamp_into_ffi")),
                CustomTypeConverter::path(Path::single("timestamp_try_from_ffi")),
            ),
        )
    }

    fn byte_slice() -> TypeExpr {
        TypeExpr::slice(TypeExpr::Primitive(Primitive::U8))
    }

    fn byte_vec() -> TypeExpr {
        TypeExpr::vec(TypeExpr::Primitive(Primitive::U8))
    }

    fn parameter(name: &str, expr: TypeExpr) -> ParameterDef {
        ParameterDef::value(CanonicalName::single(name), expr)
    }

    fn vector_parameter(name: &str, element: TypeExpr) -> ParameterDef {
        parameter(name, TypeExpr::vec(element))
    }

    fn result_return(ok: TypeExpr, error: TypeExpr) -> ReturnDef {
        ReturnDef::value(TypeExpr::result(ok, error))
    }

    fn fn_signature(parameters: Vec<TypeExpr>, returns: ReturnDef) -> FnSig {
        FnSig::new(parameters, returns)
    }

    fn fn_trait(parameters: Vec<TypeExpr>, returns: ReturnDef) -> FnTrait {
        FnTrait::new(FnTraitKind::Fn, fn_signature(parameters, returns))
    }

    fn impl_closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::impl_fn(fn_trait(parameters, returns))
    }

    fn boxed_closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::boxed(TypeExpr::dyn_fn(fn_trait(parameters, returns)))
    }

    fn function_pointer(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::fn_ptr(fn_signature(parameters, returns))
    }

    fn boxed_listener() -> TypeExpr {
        TypeExpr::boxed(TypeExpr::dyn_trait(
            TraitId::new("demo::Listener"),
            path("Listener"),
        ))
    }

    fn boxed_send_listener() -> TypeExpr {
        TypeExpr::boxed(TypeExpr::Dyn(TraitBounds::new(
            BaseTrait::Named {
                id: TraitId::new("demo::Listener"),
                path: path("Listener"),
            },
            vec![AdditionalBound::AutoTrait(Path::single("Send"))],
        )))
    }

    fn impl_listener() -> TypeExpr {
        TypeExpr::impl_trait(TraitId::new("demo::Listener"), path("Listener"))
    }

    fn arc_listener() -> TypeExpr {
        TypeExpr::arc(TypeExpr::dyn_trait(
            TraitId::new("demo::Listener"),
            path("Listener"),
        ))
    }

    fn nullable_boxed_listener() -> TypeExpr {
        TypeExpr::option(boxed_listener())
    }

    fn nullable_arc_listener() -> TypeExpr {
        TypeExpr::option(TypeExpr::arc(TypeExpr::dyn_trait(
            TraitId::new("demo::Listener"),
            path("Listener"),
        )))
    }

    fn void_source_contract() -> SourceContract {
        let function =
            FunctionDef::new(FunctionId::new("demo::ping"), CanonicalName::single("ping"));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_answer_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.execution = ExecutionKind::Async;
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_ping_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::ping"), CanonicalName::single("ping"));
        function.execution = ExecutionKind::Async;

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_greet_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::greet"),
            CanonicalName::single("greet"),
        );
        function.execution = ExecutionKind::Async;
        function.returns = ReturnDef::value(TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        function.execution = ExecutionKind::Async;
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("name"),
            TypeExpr::String,
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_borrowed_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        function.execution = ExecutionKind::Async;
        let mut parameter = ParameterDef::value(CanonicalName::single("name"), TypeExpr::String);
        parameter.passing = ParameterPassing::Ref;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_result_i32_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_count"),
            CanonicalName::single("try_count"),
        );
        function.execution = ExecutionKind::Async;
        function.returns = result_return(TypeExpr::Primitive(Primitive::I32), TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
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

    fn direct_point_record() -> RecordDef {
        let mut record = point_record();
        record.repr = ReprAttr::new(vec![ReprItem::C]);
        record
    }

    fn record_method(
        name: &str,
        receiver: Receiver,
        parameters: Vec<ParameterDef>,
        returns: ReturnDef,
    ) -> MethodDef {
        let mut method = MethodDef::new(MethodId::new(name), CanonicalName::single(name), receiver);
        method.parameters = parameters;
        method.returns = returns;
        method
    }

    fn direct_point_record_with_method(method: MethodDef) -> RecordDef {
        let mut record = direct_point_record();
        record.methods.push(method);
        record
    }

    fn profile_record_with_method(method: MethodDef) -> RecordDef {
        let mut record = profile_record();
        record.methods.push(method);
        record
    }

    fn status_enum() -> EnumDef {
        let mut enumeration =
            EnumDef::new(EnumId::new("demo::Status"), CanonicalName::single("Status"));
        enumeration.variants = vec![
            VariantDef::unit(SourceName::new("Ready", CanonicalName::single("Ready"))),
            VariantDef::unit(SourceName::new("Failed", CanonicalName::single("Failed"))),
        ];
        enumeration
    }

    fn status_enum_with_method(method: MethodDef) -> EnumDef {
        let mut enumeration = status_enum();
        enumeration.methods.push(method);
        enumeration
    }

    fn event_enum() -> EnumDef {
        let mut enumeration =
            EnumDef::new(EnumId::new("demo::Event"), CanonicalName::single("Event"));
        enumeration.variants = vec![
            VariantDef::unit(SourceName::new("Empty", CanonicalName::single("Empty"))),
            VariantDef {
                name: SourceName::new("Count", CanonicalName::single("Count")),
                discriminant: None,
                payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(Primitive::U32)]),
                doc: None,
                user_attrs: Vec::new(),
                source: Source::exported(),
                source_span: None,
            },
            VariantDef {
                name: SourceName::new("Named", CanonicalName::single("Named")),
                discriminant: None,
                payload: VariantPayload::Struct(vec![FieldDef::new(
                    CanonicalName::single("name"),
                    TypeExpr::String,
                )]),
                doc: None,
                user_attrs: Vec::new(),
                source: Source::exported(),
                source_span: None,
            },
        ];
        enumeration
    }

    fn event_enum_with_method(method: MethodDef) -> EnumDef {
        let mut enumeration = event_enum();
        enumeration.methods.push(method);
        enumeration
    }

    fn profile_record() -> RecordDef {
        let mut record = RecordDef::new("demo::Profile".into(), CanonicalName::single("Profile"));
        record.fields = vec![FieldDef::new(
            CanonicalName::single("name"),
            TypeExpr::String,
        )];
        record
    }

    fn custom_profile_record() -> RecordDef {
        let mut record = RecordDef::new("demo::Profile".into(), CanonicalName::single("Profile"));
        record.fields = vec![FieldDef::new(
            CanonicalName::single("when"),
            custom_timestamp(),
        )];
        record
    }

    fn tuple_record() -> RecordDef {
        let mut record = RecordDef::new("demo::Pair".into(), CanonicalName::single("Pair"));
        record.fields = vec![FieldDef::new(
            CanonicalName::single("values"),
            TypeExpr::tuple(vec![
                TypeExpr::Primitive(Primitive::I32),
                TypeExpr::Primitive(Primitive::I32),
            ]),
        )];
        record
    }

    fn engine_class() -> ClassDef {
        ClassDef::new("demo::Engine".into(), CanonicalName::single("Engine"))
    }

    fn engine_class_with_method(method: MethodDef) -> ClassDef {
        let mut class = engine_class();
        class.methods.push(method);
        class
    }

    fn stream(name: &str, item_type: TypeExpr) -> StreamDef {
        let mut stream = StreamDef::new(
            StreamId::new(format!("demo::Engine::{name}")),
            CanonicalName::single(name),
            item_type,
        );
        stream.owner = Some(ClassId::new("demo::Engine"));
        stream
    }

    fn engine_stream_contract(stream: StreamDef) -> SourceContract {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class());
        source.streams.push(stream);
        source
    }

    fn profile_stream_contract() -> SourceContract {
        let mut source = engine_stream_contract(stream("profiles", record("Profile")));
        source.records.push(profile_record());
        source
    }

    fn point_stream_contract() -> SourceContract {
        let mut source = engine_stream_contract(stream("points", record("Point")));
        source.records.push(point_record());
        source
    }

    fn status_stream_contract() -> SourceContract {
        let mut source = engine_stream_contract(stream("statuses", enumeration("Status")));
        source.enums.push(status_enum());
        source
    }

    fn timestamp_stream_contract() -> SourceContract {
        let mut source = engine_stream_contract(stream("timestamps", custom_timestamp()));
        source.customs.push(timestamp_custom_def());
        source
    }

    fn listener_trait() -> TraitDef {
        TraitDef::new(
            TraitId::new("demo::Listener"),
            CanonicalName::single("Listener"),
        )
    }

    fn listener_trait_with_method() -> TraitDef {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("on_value"),
            CanonicalName::single("on_value"),
            Receiver::Shared,
        );
        method.parameters = vec![parameter("value", TypeExpr::Primitive(Primitive::U32))];
        method.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));
        listener.methods.push(method);
        listener
    }

    fn listener_trait_contract() -> SourceContract {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait_with_method());
        source
    }

    fn bytes_returning_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("payload"),
            CanonicalName::single("payload"),
            Receiver::Shared,
        );
        method.returns = ReturnDef::value(byte_vec());
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn direct_returning_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut optional = MethodDef::new(
            MethodId::new("maybe_count"),
            CanonicalName::single("maybe_count"),
            Receiver::Shared,
        );
        optional.returns = ReturnDef::value(TypeExpr::option(TypeExpr::Primitive(Primitive::I32)));
        let mut vector = MethodDef::new(
            MethodId::new("numbers"),
            CanonicalName::single("numbers"),
            Receiver::Shared,
        );
        vector.returns = ReturnDef::value(TypeExpr::vec(TypeExpr::Primitive(Primitive::I32)));
        listener.methods.push(optional);
        listener.methods.push(vector);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn borrowed_string_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("on_name"),
            CanonicalName::single("on_name"),
            Receiver::Shared,
        );
        let mut parameter = parameter("name", TypeExpr::Str);
        parameter.passing = ParameterPassing::Ref;
        method.parameters = vec![parameter];
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn borrowed_u32_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("on_value"),
            CanonicalName::single("on_value"),
            Receiver::Shared,
        );
        let mut parameter = parameter("value", TypeExpr::Primitive(Primitive::U32));
        parameter.passing = ParameterPassing::Ref;
        method.parameters = vec![parameter];
        method.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn async_string_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("on_event"),
            CanonicalName::single("on_event"),
            Receiver::Shared,
        );
        method.execution = ExecutionKind::Async;
        method.parameters = vec![parameter("value", TypeExpr::Primitive(Primitive::I32))];
        method.returns = ReturnDef::value(TypeExpr::String);
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn async_direct_returning_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut count = MethodDef::new(
            MethodId::new("count"),
            CanonicalName::single("count"),
            Receiver::Shared,
        );
        count.execution = ExecutionKind::Async;
        count.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));
        let mut optional = MethodDef::new(
            MethodId::new("maybe_count"),
            CanonicalName::single("maybe_count"),
            Receiver::Shared,
        );
        optional.execution = ExecutionKind::Async;
        optional.returns = ReturnDef::value(TypeExpr::option(TypeExpr::Primitive(Primitive::I32)));
        let mut vector = MethodDef::new(
            MethodId::new("numbers"),
            CanonicalName::single("numbers"),
            Receiver::Shared,
        );
        vector.execution = ExecutionKind::Async;
        vector.returns = ReturnDef::value(TypeExpr::vec(TypeExpr::Primitive(Primitive::I32)));
        listener.methods.push(count);
        listener.methods.push(optional);
        listener.methods.push(vector);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn async_fallible_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut count = MethodDef::new(
            MethodId::new("try_count"),
            CanonicalName::single("try_count"),
            Receiver::Shared,
        );
        count.execution = ExecutionKind::Async;
        count.returns = result_return(TypeExpr::Primitive(Primitive::U32), TypeExpr::String);
        let mut numbers = MethodDef::new(
            MethodId::new("try_numbers"),
            CanonicalName::single("try_numbers"),
            Receiver::Shared,
        );
        numbers.execution = ExecutionKind::Async;
        numbers.returns = result_return(byte_vec(), TypeExpr::String);
        listener.methods.push(count);
        listener.methods.push(numbers);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn async_callback_returning_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("listener"),
            CanonicalName::single("listener"),
            Receiver::Shared,
        );
        method.execution = ExecutionKind::Async;
        method.returns = ReturnDef::value(impl_listener());
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn closure_taking_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("on_render"),
            CanonicalName::single("on_render"),
            Receiver::Shared,
        );
        method.parameters = vec![parameter(
            "callback",
            impl_closure(
                vec![TypeExpr::Primitive(Primitive::U32)],
                ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
            ),
        )];
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn closure_returning_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("make_handler"),
            CanonicalName::single("make_handler"),
            Receiver::Shared,
        );
        method.returns = ReturnDef::value(impl_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn boxed_closure_returning_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("make_handler"),
            CanonicalName::single("make_handler"),
            Receiver::Shared,
        );
        method.returns = ReturnDef::value(boxed_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn string_closure_taking_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("on_render"),
            CanonicalName::single("on_render"),
            Receiver::Shared,
        );
        method.parameters = vec![parameter(
            "callback",
            impl_closure(vec![TypeExpr::String], ReturnDef::value(TypeExpr::String)),
        )];
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn fallible_callback_returning_listener_contract() -> SourceContract {
        let mut listener = listener_trait();
        let mut method = MethodDef::new(
            MethodId::new("listener"),
            CanonicalName::single("listener"),
            Receiver::Shared,
        );
        method.returns = result_return(boxed_listener(), TypeExpr::String);
        listener.methods.push(method);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener);
        source
    }

    fn closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                vec![TypeExpr::Primitive(Primitive::U32)],
                ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
            ),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn string_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(vec![TypeExpr::String], ReturnDef::value(TypeExpr::String)),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn custom_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(Vec::<TypeExpr>::new(), ReturnDef::value(custom_timestamp())),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn fallible_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                result_return(TypeExpr::Unit, TypeExpr::String),
            ),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn fallible_custom_sequence_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                result_return(TypeExpr::vec(custom_timestamp()), TypeExpr::String),
            ),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn fallible_i32_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                result_return(TypeExpr::Primitive(Primitive::I32), TypeExpr::String),
            ),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn fallible_string_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                result_return(TypeExpr::String, TypeExpr::String),
            ),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn optional_scalar_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                ReturnDef::value(TypeExpr::option(TypeExpr::Primitive(Primitive::I32))),
            ),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn direct_vector_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                ReturnDef::value(TypeExpr::Vec(Box::new(TypeExpr::Primitive(Primitive::U32)))),
            ),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_callback"),
            CanonicalName::single("make_callback"),
        );
        function.returns = ReturnDef::value(impl_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_boxed_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_callback"),
            CanonicalName::single("make_callback"),
        );
        function.execution = ExecutionKind::Async;
        function.returns = ReturnDef::value(boxed_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn closure_return_with_record_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_callback"),
            CanonicalName::single("make_callback"),
        );
        function.returns = ReturnDef::value(impl_closure(
            vec![record("Point")],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn closure_return_with_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_runner"),
            CanonicalName::single("make_runner"),
        );
        let callback = boxed_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        function.returns = ReturnDef::value(impl_closure(
            vec![callback],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn function_pointer_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_callback"),
            CanonicalName::single("make_callback"),
        );
        function.returns = ReturnDef::value(function_pointer(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn string_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_mapper"),
            CanonicalName::single("make_mapper"),
        );
        function.returns = ReturnDef::value(impl_closure(
            vec![TypeExpr::String],
            ReturnDef::value(TypeExpr::String),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn custom_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_clock"),
            CanonicalName::single("make_clock"),
        );
        function.returns = ReturnDef::value(impl_closure(
            Vec::<TypeExpr>::new(),
            ReturnDef::value(custom_timestamp()),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn fallible_i32_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_callback"),
            CanonicalName::single("make_callback"),
        );
        function.returns = ReturnDef::value(impl_closure(
            Vec::<TypeExpr>::new(),
            result_return(TypeExpr::Primitive(Primitive::I32), TypeExpr::String),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn fallible_string_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_mapper"),
            CanonicalName::single("make_mapper"),
        );
        function.returns = ReturnDef::value(impl_closure(
            Vec::<TypeExpr>::new(),
            result_return(TypeExpr::String, TypeExpr::String),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn result_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_make_callback"),
            CanonicalName::single("try_make_callback"),
        );
        let closure = impl_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        function.returns = result_return(closure, TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn direct_record_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::norm"), CanonicalName::single("norm"));
        function.parameters = vec![parameter("point", record("Point"))];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::F64));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn mutable_direct_record_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::shift"),
            CanonicalName::single("shift"),
        );
        let mut parameter = parameter("point", record("Point"));
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::F64));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn mutable_direct_primitive_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::bump"), CanonicalName::single("bump"));
        let mut parameter = parameter("count", TypeExpr::Primitive(Primitive::I32));
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn direct_record_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::origin"),
            CanonicalName::single("origin"),
        );
        function.returns = ReturnDef::value(record("Point"));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn string_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::greet"),
            CanonicalName::single("greet"),
        );
        function.returns = ReturnDef::value(TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn bytes_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::payload"),
            CanonicalName::single("payload"),
        );
        function.returns = ReturnDef::value(byte_vec());

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("name"),
            TypeExpr::String,
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn borrowed_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        let mut parameter = parameter("name", TypeExpr::Str);
        parameter.passing = ParameterPassing::Ref;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn mutable_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::rewrite"),
            CanonicalName::single("rewrite"),
        );
        let mut parameter = parameter("name", TypeExpr::Str);
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn bytes_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::bytes_len"),
            CanonicalName::single("bytes_len"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("bytes"),
            byte_vec(),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn mutable_bytes_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::fill"), CanonicalName::single("fill"));
        let mut parameter = parameter("bytes", byte_slice());
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn mutable_byte_vec_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::grow"), CanonicalName::single("grow"));
        let mut parameter = parameter("bytes", byte_vec());
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::Void;

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn encoded_record_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_score"),
            CanonicalName::single("name_score"),
        );
        function.parameters = vec![parameter("profile", record("Profile"))];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record());
        source.functions.push(function);
        source
    }

    fn mutable_encoded_record_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::rename"),
            CanonicalName::single("rename"),
        );
        let mut parameter = parameter("profile", record("Profile"));
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record());
        source.functions.push(function);
        source
    }

    fn option_i32_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::set_count"),
            CanonicalName::single("set_count"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("count"),
            TypeExpr::option(TypeExpr::Primitive(Primitive::I32)),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn vec_u32_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::sum"), CanonicalName::single("sum"));
        function.parameters = vec![vector_parameter(
            "values",
            TypeExpr::Primitive(Primitive::U32),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn vec_point_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::count_points"),
            CanonicalName::single("count_points"),
        );
        function.parameters = vec![vector_parameter("points", record("Point"))];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn result_i32_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_count"),
            CanonicalName::single("try_count"),
        );
        function.returns = result_return(TypeExpr::Primitive(Primitive::I32), TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn result_unit_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_ping"),
            CanonicalName::single("try_ping"),
        );
        function.returns = result_return(TypeExpr::Unit, TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn result_string_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_greet"),
            CanonicalName::single("try_greet"),
        );
        function.returns = result_return(TypeExpr::String, TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn class_param_nullable_return_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::open"), CanonicalName::single("open"));
        function.parameters = vec![parameter("engine", class("Engine"))];
        function.returns = ReturnDef::value(TypeExpr::option(class("Engine")));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class());
        source.functions.push(function);
        source
    }

    fn boxed_callback_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::listen"),
            CanonicalName::single("listen"),
        );
        function.parameters = vec![parameter("listener", boxed_listener())];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn boxed_send_callback_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::listen"),
            CanonicalName::single("listen"),
        );
        function.parameters = vec![parameter("listener", boxed_send_listener())];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn impl_callback_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::listen"),
            CanonicalName::single("listen"),
        );
        function.parameters = vec![parameter("listener", impl_listener())];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait_with_method());
        source.functions.push(function);
        source
    }

    fn async_impl_callback_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::listen"),
            CanonicalName::single("listen"),
        );
        function.execution = ExecutionKind::Async;
        function.parameters = vec![parameter("listener", impl_listener())];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait_with_method());
        source.functions.push(function);
        source
    }

    fn nullable_arc_callback_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::maybe"),
            CanonicalName::single("maybe"),
        );
        function.parameters = vec![parameter("listener", nullable_arc_listener())];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn boxed_callback_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_listener"),
            CanonicalName::single("make_listener"),
        );
        function.returns = ReturnDef::value(boxed_listener());

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn nullable_arc_callback_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::maybe_listener"),
            CanonicalName::single("maybe_listener"),
        );
        function.returns = ReturnDef::value(nullable_arc_listener());

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn arc_callback_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::shared_listener"),
            CanonicalName::single("shared_listener"),
        );
        function.returns = ReturnDef::value(arc_listener());

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn nullable_boxed_callback_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::maybe_boxed_listener"),
            CanonicalName::single("maybe_boxed_listener"),
        );
        function.returns = ReturnDef::value(nullable_boxed_listener());

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn result_boxed_callback_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_make_listener"),
            CanonicalName::single("try_make_listener"),
        );
        function.returns = result_return(boxed_listener(), TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn borrowed_class_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::engine_id"),
            CanonicalName::single("engine_id"),
        );
        let mut parameter = parameter("engine", class("Engine"));
        parameter.passing = ParameterPassing::Ref;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class());
        source.functions.push(function);
        source
    }

    fn result_class_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_open"),
            CanonicalName::single("try_open"),
        );
        function.returns = result_return(class("Engine"), TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class());
        source.functions.push(function);
        source
    }

    fn option_i32_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::maybe_count"),
            CanonicalName::single("maybe_count"),
        );
        function.returns = ReturnDef::value(TypeExpr::option(TypeExpr::Primitive(Primitive::I32)));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn vec_i32_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::numbers"),
            CanonicalName::single("numbers"),
        );
        function.returns = ReturnDef::value(TypeExpr::vec(TypeExpr::Primitive(Primitive::I32)));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn custom_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::year"), CanonicalName::single("year"));
        function.parameters = vec![parameter("when", custom_timestamp())];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn custom_tuple_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::tuple_year"),
            CanonicalName::single("tuple_year"),
        );
        function.parameters = vec![parameter(
            "value",
            TypeExpr::tuple(vec![
                custom_timestamp(),
                TypeExpr::Primitive(Primitive::U32),
            ]),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn custom_map_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::map_years"),
            CanonicalName::single("map_years"),
        );
        function.parameters = vec![parameter(
            "values",
            TypeExpr::hash_map(TypeExpr::String, custom_timestamp()),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn custom_map_key_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::map_years"),
            CanonicalName::single("map_years"),
        );
        function.parameters = vec![parameter(
            "values",
            TypeExpr::hash_map(custom_timestamp(), TypeExpr::Primitive(Primitive::U32)),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn custom_map_key_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::map_years"),
            CanonicalName::single("map_years"),
        );
        function.returns = ReturnDef::value(TypeExpr::hash_map(
            custom_timestamp(),
            TypeExpr::Primitive(Primitive::U32),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn floating_point_map_key_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::sum_values"),
            CanonicalName::single("sum_values"),
        );
        function.parameters = vec![parameter(
            "values",
            TypeExpr::hash_map(
                TypeExpr::Primitive(Primitive::F32),
                TypeExpr::Primitive(Primitive::U32),
            ),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn oversized_tuple_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::sum_tuple"),
            CanonicalName::single("sum_tuple"),
        );
        function.parameters = vec![parameter(
            "values",
            TypeExpr::tuple(std::iter::repeat_n(TypeExpr::Primitive(Primitive::U32), 13).collect()),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn custom_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::stamp"),
            CanonicalName::single("stamp"),
        );
        function.returns = ReturnDef::value(custom_timestamp());

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn custom_result_error_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_stamp"),
            CanonicalName::single("try_stamp"),
        );
        function.returns = result_return(TypeExpr::Primitive(Primitive::U32), custom_timestamp());

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    fn nested_custom_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::timeline"),
            CanonicalName::single("timeline"),
        );
        function.returns = ReturnDef::value(TypeExpr::vec(TypeExpr::option(custom_timestamp())));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.functions.push(function);
        source
    }

    #[test]
    fn native_inline_constant_expansion_emits_no_wrapper() {
        let source = constant_contract(
            "demo::ANSWER",
            "ANSWER",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(42, "42"))),
        );
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_constant(&expansion, &source.constants[0]).expect("expanded constant");

        assert!(tokens.is_empty());
    }

    #[test]
    fn native_bytes_constant_expansion_emits_accessor() {
        let source = constant_contract(
            "demo::MAGIC",
            "MAGIC",
            byte_slice(),
            ConstExpr::Literal(Literal::Bytes(b"ffi".to_vec())),
        );
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_constant(&expansion, &source.constants[0]).expect("expanded constant");
        let generated = quote! {
            pub const MAGIC: &[u8] = b"ffi";

            #tokens
        };

        syn::parse2::<syn::File>(generated.clone()).expect("bytes constant expansion parses");
        assert_generated_crate_checks("native_bytes_constant", generated);
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_const_demo_magic"));
        assert!(rendered.contains("let __boltffi_result : & [u8] = MAGIC"));
        assert!(rendered.contains("FfiBuf :: wire_encode (& __boltffi_result)"));
    }

    #[test]
    fn wasm_bytes_constant_expansion_emits_packed_accessor() {
        let source = constant_contract(
            "demo::MAGIC",
            "MAGIC",
            byte_slice(),
            ConstExpr::Literal(Literal::Bytes(b"ffi".to_vec())),
        );
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_constant(&expansion, &source.constants[0]).expect("expanded constant");
        let generated = quote! {
            pub const MAGIC: &[u8] = b"ffi";

            #tokens
        };

        syn::parse2::<syn::File>(generated.clone()).expect("wasm bytes constant expansion parses");
        assert_generated_crate_checks_target(
            "wasm_bytes_constant",
            "wasm32-unknown-unknown",
            generated,
        );
        let rendered = tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_const_demo_magic"));
        assert!(rendered.contains(") -> u64"));
        assert!(rendered.contains("let __boltffi_result : & [u8] = MAGIC"));
        assert!(rendered.contains("into_packed"));
    }

    #[test]
    fn native_custom_slice_constant_expansion_borrows_elements_before_conversion() {
        let mut source = constant_contract(
            "demo::TIMES",
            "TIMES",
            TypeExpr::slice(custom_timestamp()),
            ConstExpr::Raw("& []".to_owned()),
        );
        source.customs.push(timestamp_custom_def());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_constant(&expansion, &source.constants[0]).expect("expanded constant");
        let generated = quote! {
            pub struct Timestamp(i64);

            pub const TIMES: &[Timestamp] = &[];

            pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                value.0
            }

            pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                Ok(Timestamp(value))
            }

            #tokens
        };

        assert_generated_crate_checks("native_custom_slice_constant", generated);
        let rendered = tokens.to_string();
        assert!(rendered.contains("let __boltffi_result = TIMES"));
        assert!(rendered.contains("(timestamp_into_ffi) (value)"));
        assert!(!rendered.contains("(timestamp_into_ffi) (& value)"));
    }

    #[test]
    fn function_expansion_uses_exact_source_declaration() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax: ItemFn = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn function_expansion_invokes_source_name_spelling() {
        let source = source_name_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn syntax_payload() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn syntax_payload() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_http_request() -> u32 {
                    HTTPRequest()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn function_expansion_uses_source_parameter_name_spelling() {
        let source = source_parameter_name_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn syntax_payload(value: u32) -> u32 {
                value
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn syntax_payload(value: u32) -> u32 {
                    value
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_syntax_payload(
                    HTTPCode: u32
                ) -> u32 {
                    syntax_payload(HTTPCode)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn function_expansion_uses_private_source_visibility() {
        let source = source_visibility_contract(Visibility::Private);
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax: ItemFn = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_custom_param_expansion_decodes_repr_and_calls_try_from_ffi() {
        let source = custom_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn year(when: Timestamp) -> u32 {
                when.year()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn year(when: Timestamp) -> u32 {
                    when.year()
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_year(
                    __boltffi_when_ptr: *const u8,
                    __boltffi_when_len: usize
                ) -> u32 {
                    let when = {
                        if __boltffi_when_ptr.is_null() && __boltffi_when_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(when),
                                __boltffi_when_len
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_when_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_when_ptr,
                                    __boltffi_when_len
                                )
                            }
                        };
                        let __boltffi_decoded = match ::boltffi::__private::wire::decode::<i64>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(when),
                                    error,
                                    __boltffi_when_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        };
                        match (|__boltffi_value: i64| (timestamp_try_from_ffi)(__boltffi_value))(
                            __boltffi_decoded
                        ) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: custom conversion failed: {:?} (buf_len={})",
                                    stringify!(when),
                                    error,
                                    __boltffi_when_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        }
                    };
                    year(when)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_custom_tuple_param_expansion_decodes_repr_and_calls_try_from_ffi() {
        let source = custom_tuple_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn tuple_year(value: (Timestamp, u32)) -> u32 {
                value.0.year() + value.1
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_generated_crate_checks(
            "native_custom_tuple_param",
            quote! {
                pub struct Timestamp(i64);

                impl Timestamp {
                    fn year(&self) -> u32 {
                        self.0 as u32
                    }
                }

                pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                    value.0
                }

                pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                    Ok(Timestamp(value))
                }

                #tokens
            },
        );
    }

    #[test]
    fn native_custom_map_param_expansion_decodes_repr_and_calls_try_from_ffi() {
        let source = custom_map_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn map_years(values: HashMap<String, Timestamp>) -> u32 {
                values.into_values().map(|value| value.year()).sum()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_generated_crate_checks(
            "native_custom_map_param",
            quote! {
                use std::collections::HashMap;

                pub struct Timestamp(i64);

                impl Timestamp {
                    fn year(&self) -> u32 {
                        self.0 as u32
                    }
                }

                pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                    value.0
                }

                pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                    Ok(Timestamp(value))
                }

                #tokens
            },
        );

        let rendered = tokens.to_string();
        assert!(
            rendered.contains(
                "wire :: decode :: < :: std :: collections :: HashMap < String , i64 > >"
            )
        );
        assert!(!rendered.contains("Vec < (String , i64) >"));
    }

    #[test]
    fn native_custom_map_key_param_expansion_rejects_identity_losing_conversion() {
        let source = custom_map_key_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn map_years(values: HashMap<Timestamp, u32>) -> u32 {
                values.into_values().sum()
            }
        };

        let error = expand_function(&expansion, &source.functions[0], syntax)
            .expect_err("custom map keys must reject");

        assert!(matches!(
            error,
            Error::UnsupportedExpansion("custom encoded map key")
        ));
    }

    #[test]
    fn native_custom_map_key_return_expansion_rejects_identity_losing_conversion() {
        let source = custom_map_key_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn map_years() -> HashMap<Timestamp, u32> {
                HashMap::new()
            }
        };

        let error = expand_function(&expansion, &source.functions[0], syntax)
            .expect_err("custom map keys must reject");

        assert!(matches!(
            error,
            Error::UnsupportedExpansion("custom encoded map key")
        ));
    }

    #[test]
    fn native_floating_point_map_key_param_expansion_rejects_generated_trait_failure() {
        let source = floating_point_map_key_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn sum_values(values: HashMap<f32, u32>) -> u32 {
                values.into_values().sum()
            }
        };

        let error = expand_function(&expansion, &source.functions[0], syntax)
            .expect_err("floating-point map keys must reject");

        assert!(matches!(
            error,
            Error::UnsupportedExpansion("floating-point encoded map key")
        ));
    }

    #[test]
    fn native_oversized_tuple_param_expansion_rejects_missing_runtime_wire_impl() {
        let source = oversized_tuple_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn sum_tuple(values: (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32)) -> u32 {
                values.0
            }
        };

        let error = expand_function(&expansion, &source.functions[0], syntax)
            .expect_err("oversized tuple must reject");

        assert!(matches!(
            error,
            Error::UnsupportedExpansion("encoded tuple arity")
        ));
    }

    #[test]
    fn native_mutable_byte_vec_param_expansion_rejects_unsupported_growable_param() {
        let source = mutable_byte_vec_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn grow(bytes: &mut Vec<u8>) {}
        };

        let error = expand_function(&expansion, &source.functions[0], syntax)
            .expect_err("`&mut Vec<u8>` params must reject");

        assert!(matches!(
            error,
            Error::UnsupportedExpansion(
                "`&mut Vec<u8>` parameters are not supported; \
                 use `&mut [u8]` for in-place mutation or return `Vec<u8>`"
            )
        ));
    }

    #[test]
    fn native_direct_record_expansion_emits_raw_memory_traits() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(direct_point_record());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        syn::parse2::<syn::File>(quote! {
            #[repr(C)]
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }
            #tokens
        })
        .expect("direct record expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("unsafe impl :: boltffi :: __private :: Passable for Point"));
        assert!(
            rendered.contains("unsafe impl :: boltffi :: __private :: wire :: Blittable for Point")
        );
        assert!(rendered.contains("impl :: boltffi :: __private :: VecTransport for Point"));
        assert!(rendered.contains(
            "const _ : [() ; 8usize] = [() ; :: core :: mem :: size_of :: < Point > ()] ;"
        ));
        assert!(rendered.contains(
            "const _ : [() ; 8usize] = [() ; :: core :: mem :: align_of :: < Point > ()] ;"
        ));
    }

    #[test]
    fn native_encoded_record_expansion_emits_wire_traits() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        syn::parse2::<syn::File>(quote! {
            pub struct Profile {
                pub name: String,
            }
            #tokens
        })
        .expect("encoded record expansion parses");
        let rendered = tokens.to_string();
        assert!(
            rendered.contains("unsafe impl :: boltffi :: __private :: WirePassable for Profile")
        );
        assert!(
            rendered.contains("impl :: boltffi :: __private :: wire :: WireEncode for Profile")
        );
        assert!(
            rendered.contains("impl :: boltffi :: __private :: wire :: WireDecode for Profile")
        );
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: WireEncode :: wire_size (& self . name)"
        ));
        assert!(rendered.contains("let name : String = __boltffi_name_decoded ;"));
    }

    #[test]
    fn native_encoded_record_expansion_converts_custom_fields_inside_wire_traits() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.records.push(custom_profile_record());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        syn::parse2::<syn::File>(quote! {
            pub struct Timestamp(i64);

            pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                value.0
            }

            pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                Ok(Timestamp(value))
            }

            pub struct Profile {
                pub when: Timestamp,
            }

            #tokens
        })
        .expect("custom encoded record expansion parses");
        let rendered = tokens.to_string();
        assert!(
            rendered.contains("let __boltffi_when_wire = (timestamp_into_ffi) (& self . when) ;")
        );
        assert!(
            rendered.contains(
                "< i64 as :: boltffi :: __private :: wire :: WireDecode > :: decode_from"
            )
        );
        assert!(rendered.contains(
            "match (| __boltffi_value : i64 | (timestamp_try_from_ffi) (__boltffi_value)) (__boltffi_when_decoded)"
        ));
        assert!(
            !rendered.contains("< Timestamp as :: boltffi :: __private :: wire :: WireDecode >")
        );
    }

    #[test]
    fn native_direct_record_expansion_emits_initializer_wrapper() {
        let initializer = record_method(
            "new",
            Receiver::None,
            vec![parameter("x", TypeExpr::Primitive(Primitive::F64))],
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source
            .records
            .push(direct_point_record_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        syn::parse2::<syn::File>(quote! {
            #[repr(C)]
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }

            impl Point {
                pub fn new(x: f64) -> Self {
                    Self { x }
                }
            }

            #tokens
        })
        .expect("direct record initializer expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_record_demo_point_new"));
        assert!(rendered.contains("Point :: new (x)"));
    }

    #[test]
    fn native_direct_record_expansion_emits_static_method_wrapper() {
        let method = record_method(
            "origin_x",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(direct_point_record_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        syn::parse2::<syn::File>(quote! {
            #[repr(C)]
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }

            impl Point {
                pub fn origin_x() -> f64 {
                    0.0
                }
            }

            #tokens
        })
        .expect("direct record static method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_record_demo_point_origin_x"));
        assert!(!rendered.contains("fn boltffi_init_record_demo_point_origin_x"));
        assert!(!rendered.contains("__boltffi_receiver"));
        assert!(rendered.contains("Point :: origin_x ()"));
    }

    #[test]
    fn native_direct_record_expansion_emits_async_initializer_wrapper() {
        let mut initializer = record_method(
            "load",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        initializer.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source
            .records
            .push(direct_point_record_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let generated = quote! {
            #[repr(C)]
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }

            impl Point {
                pub async fn load() -> Self {
                    Self { x: 1.0 }
                }
            }

            #tokens
        };
        syn::parse2::<syn::File>(generated.clone())
            .expect("direct record async initializer parses");
        assert_generated_crate_checks("native_direct_record_async_initializer", generated);
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_record_demo_point_load"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: rustfuture :: rust_future_new (async move { Point :: load () . await })"
        ));
        assert!(rendered.contains("fn boltffi_async_init_record_demo_point_load_poll"));
        assert!(rendered.contains("fn boltffi_async_init_record_demo_point_load_complete"));
    }

    #[test]
    fn native_direct_record_expansion_emits_instance_method_wrapper() {
        let method = record_method(
            "norm",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(direct_point_record_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_record_demo_point_norm"));
        assert!(rendered.contains(
            "__boltffi_receiver : < Point as :: boltffi :: __private :: Passable > :: In"
        ));
        assert!(rendered.contains(
            "let __boltffi_receiver : Point = unsafe { < Point as :: boltffi :: __private :: Passable > :: unpack (__boltffi_receiver) } ;"
        ));
        assert!(rendered.contains("__boltffi_receiver . norm ()"));
    }

    #[test]
    fn native_direct_record_method_returning_self_renders_concrete_record_type() {
        let method = record_method(
            "copy",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(direct_point_record_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        syn::parse2::<syn::File>(quote! {
            #[repr(C)]
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }

            impl Point {
                pub fn copy(&self) -> Self {
                    *self
                }
            }

            #tokens
        })
        .expect("direct record self-returning method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_record_demo_point_copy"));
        assert!(rendered.contains("-> < Point as :: boltffi :: __private :: Passable > :: Out"));
        assert!(!rendered.contains("< Self as :: boltffi :: __private :: Passable >"));
        assert!(rendered.contains("__boltffi_receiver . copy ()"));
    }

    #[test]
    fn native_direct_record_expansion_emits_async_instance_method_wrapper() {
        let mut method = record_method(
            "compute",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
        );
        method.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(direct_point_record_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        syn::parse2::<syn::File>(quote! {
            #[repr(C)]
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }

            impl Point {
                pub async fn compute(&self) -> f64 {
                    self.x
                }
            }

            #tokens
        })
        .expect("direct record async method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_record_demo_point_compute"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: rustfuture :: rust_future_new (async move { __boltffi_receiver . compute () . await })"
        ));
        assert!(rendered.contains("fn boltffi_async_method_record_demo_point_compute_poll"));
    }

    #[test]
    fn wasm_direct_record_expansion_emits_async_instance_method_wrapper() {
        let mut method = record_method(
            "compute",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
        );
        method.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(direct_point_record_with_method(method));
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        syn::parse2::<syn::File>(quote! {
            #[repr(C)]
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }

            impl Point {
                pub async fn compute(&self) -> f64 {
                    self.x
                }
            }

            #tokens
        })
        .expect("wasm direct record async method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_method_record_demo_point_compute"));
        assert!(rendered.contains("__boltffi_receiver : * const u8"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: rustfuture :: rust_future_new (async move { __boltffi_receiver . compute () . await })"
        ));
        assert!(rendered.contains("fn boltffi_async_method_record_demo_point_compute_poll_sync"));
    }

    #[test]
    fn native_direct_record_expansion_writes_mutable_receiver_back() {
        let method = record_method(
            "shift",
            Receiver::Mutable,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(direct_point_record_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_record_demo_point_shift"));
        assert!(rendered.contains(
            "__boltffi_receiver : < Point as :: boltffi :: __private :: Passable > :: In"
        ));
        assert!(rendered.contains(
            "__boltffi_receiver_out : * mut < Point as :: boltffi :: __private :: Passable > :: In"
        ));
        assert!(rendered.contains("receiver writeback pointer is null"));
        assert!(rendered.contains("__boltffi_receiver . shift ()"));
        assert!(rendered.contains(
            ":: core :: ptr :: write_unaligned (__boltffi_receiver_out , < Point as :: boltffi :: __private :: Passable > :: pack (__boltffi_receiver))"
        ));
    }

    #[test]
    fn wasm_direct_record_expansion_writes_mutable_receiver_back() {
        let method = record_method(
            "shift",
            Receiver::Mutable,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(direct_point_record_with_method(method));
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_record_demo_point_shift"));
        assert!(rendered.contains("__boltffi_receiver : * mut u8"));
        assert!(rendered.contains("let __boltffi_receiver_out = __boltffi_receiver ;"));
        assert!(rendered.contains("__boltffi_receiver . shift ()"));
        assert!(rendered.contains(
            ":: core :: ptr :: write_unaligned (__boltffi_receiver_out as * mut < Point as :: boltffi :: __private :: Passable > :: In"
        ));
    }

    #[test]
    fn native_encoded_record_expansion_emits_static_initializer_wrapper() {
        let initializer = record_method(
            "from_name",
            Receiver::None,
            vec![parameter("name", TypeExpr::String)],
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_record_demo_profile_from_name"));
        assert!(rendered.contains("Profile :: from_name (name)"));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: wire_encode"));
    }

    #[test]
    fn native_encoded_record_expansion_emits_fallible_initializer_success_out() {
        let initializer = record_method(
            "try_from_name",
            Receiver::None,
            vec![parameter("name", TypeExpr::String)],
            result_return(TypeExpr::SelfType, TypeExpr::String),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_record_demo_profile_try_from_name"));
        assert!(
            rendered.contains("__boltffi_return_out : * mut :: boltffi :: __private :: FfiBuf")
        );
        assert!(rendered.contains("Profile :: try_from_name (name)"));
        assert!(rendered.contains(
            ":: core :: ptr :: write (__boltffi_return_out , :: boltffi :: __private :: FfiBuf :: wire_encode"
        ));
    }

    #[test]
    fn native_encoded_record_expansion_emits_instance_method_wrapper() {
        let method = record_method(
            "display_name",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::String),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_record_demo_profile_display_name"));
        assert!(rendered.contains("__boltffi_receiver_ptr : * const u8"));
        assert!(rendered.contains("__boltffi_receiver_len : usize"));
        assert!(rendered.contains("let __boltffi_receiver_storage : Profile ="));
        assert!(rendered.contains("let __boltffi_receiver = & __boltffi_receiver_storage ;"));
        assert!(rendered.contains("__boltffi_receiver . display_name ()"));
    }

    #[test]
    fn native_encoded_record_expansion_emits_async_instance_method_wrapper() {
        let mut method = record_method(
            "display_name",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::String),
        );
        method.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let generated = quote! {
            pub struct Profile {
                pub name: String,
            }

            impl Profile {
                pub async fn display_name(&self) -> String {
                    self.name.clone()
                }
            }

            #tokens
        };
        syn::parse2::<syn::File>(generated.clone())
            .expect("encoded record async method expansion parses");
        assert_generated_crate_checks("native_encoded_record_async_method", generated);
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_record_demo_profile_display_name"));
        assert!(rendered.contains("fn boltffi_async_method_record_demo_profile_display_name_poll"));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: rustfuture :: rust_future_new (async move")
        );
    }

    #[test]
    fn wasm_encoded_record_expansion_emits_instance_method_wrapper() {
        let method = record_method(
            "display_name",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::String),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record_with_method(method));
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");

        let rendered = tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_method_record_demo_profile_display_name"));
        assert!(rendered.contains("__boltffi_receiver_ptr : * const u8"));
        assert!(rendered.contains("__boltffi_receiver_len : usize"));
        assert!(rendered.contains("__boltffi_receiver . display_name ()"));
        assert!(rendered.contains("into_packed"));
    }

    #[test]
    fn native_c_style_enum_expansion_emits_scalar_transport_traits() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(status_enum());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Status {
                Ready,
                Failed,
            }

            #tokens
        })
        .expect("c-style enum expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("unsafe impl :: boltffi :: __private :: Passable for Status"));
        assert!(rendered.contains("type In = i32"));
        assert!(rendered.contains("impl :: boltffi :: __private :: wire :: WireEncode for Status"));
        assert!(rendered.contains("impl :: boltffi :: __private :: wire :: WireDecode for Status"));
        assert!(rendered.contains("impl :: boltffi :: __private :: VecTransport for Status"));
        assert!(rendered.contains("Status :: Ready as i32"));
        assert!(rendered.contains("Status :: Failed as i32"));
        assert!(rendered.contains("Ok (Status :: Ready)"));
        assert!(rendered.contains("Ok (Status :: Failed)"));
        assert!(rendered.contains("InvalidWireValue :: EnumTag"));
    }

    #[test]
    fn native_c_style_enum_expansion_emits_instance_method_wrapper() {
        let method = record_method(
            "is_ready",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::Bool)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(status_enum_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Status {
                Ready,
                Failed,
            }

            impl Status {
                pub fn is_ready(&self) -> bool {
                    matches!(self, Self::Ready)
                }
            }

            #tokens
        })
        .expect("c-style enum method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_enum_demo_status_is_ready"));
        assert!(rendered.contains(
            "__boltffi_receiver : < Status as :: boltffi :: __private :: Passable > :: In"
        ));
        assert!(rendered.contains("__boltffi_receiver . is_ready ()"));
    }

    #[test]
    fn native_c_style_enum_expansion_emits_static_method_wrapper() {
        let method = record_method(
            "count",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(status_enum_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Status {
                Ready,
                Failed,
            }

            impl Status {
                pub fn count() -> u32 {
                    2
                }
            }

            #tokens
        })
        .expect("c-style enum static method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_enum_demo_status_count"));
        assert!(!rendered.contains("fn boltffi_init_enum_demo_status_count"));
        assert!(!rendered.contains("__boltffi_receiver"));
        assert!(rendered.contains("Status :: count ()"));
    }

    #[test]
    fn native_c_style_enum_expansion_emits_initializer_wrapper() {
        let initializer = record_method(
            "default_status",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(status_enum_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Status {
                Ready,
                Failed,
            }

            impl Status {
                pub fn default_status() -> Self {
                    Self::Ready
                }
            }

            #tokens
        })
        .expect("c-style enum initializer expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_enum_demo_status_default_status"));
        assert!(!rendered.contains("fn boltffi_method_enum_demo_status_default_status"));
        assert!(rendered.contains("Status :: default_status ()"));
        assert!(rendered.contains("-> < Status as :: boltffi :: __private :: Passable > :: Out"));
    }

    #[test]
    fn native_c_style_enum_expansion_emits_async_initializer_wrapper() {
        let mut initializer = record_method(
            "load",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        initializer.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(status_enum_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Status {
                Ready,
                Failed,
            }

            impl Status {
                pub async fn load() -> Self {
                    Self::Ready
                }
            }

            #tokens
        })
        .expect("c-style enum async initializer expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_enum_demo_status_load"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: rustfuture :: rust_future_new (async move { Status :: load () . await })"
        ));
        assert!(rendered.contains("fn boltffi_async_init_enum_demo_status_load_poll"));
        assert!(rendered.contains("fn boltffi_async_init_enum_demo_status_load_complete"));
    }

    #[test]
    fn wasm_c_style_enum_expansion_emits_async_initializer_wrapper() {
        let mut initializer = record_method(
            "load",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        initializer.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(status_enum_with_method(initializer));
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        let rendered = tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_init_enum_demo_status_load"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: rustfuture :: rust_future_new (async move { Status :: load () . await })"
        ));
        assert!(rendered.contains("fn boltffi_async_init_enum_demo_status_load_poll_sync"));
        assert!(rendered.contains("fn boltffi_async_init_enum_demo_status_load_complete"));
    }

    #[test]
    fn native_c_style_enum_method_returning_self_renders_concrete_enum_type() {
        let method = record_method(
            "clone_status",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(status_enum_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_enum_demo_status_clone_status"));
        assert!(rendered.contains("-> < Status as :: boltffi :: __private :: Passable > :: Out"));
        assert!(rendered.contains("__boltffi_receiver . clone_status ()"));
    }

    #[test]
    fn native_data_enum_expansion_emits_wire_traits_for_payload_variants() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(event_enum());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Event {
                Empty,
                Count(u32),
                Named { name: String },
            }

            #tokens
        })
        .expect("data enum expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("unsafe impl :: boltffi :: __private :: WirePassable for Event"));
        assert!(rendered.contains("impl :: boltffi :: __private :: wire :: WireEncode for Event"));
        assert!(rendered.contains("impl :: boltffi :: __private :: wire :: WireDecode for Event"));
        assert!(rendered.contains("Event :: Empty => 4usize"));
        assert!(rendered.contains("Event :: Count (__boltffi_payload0)"));
        assert!(rendered.contains("Event :: Named { name }"));
        assert!(
            rendered
                .contains("buffer [0 .. 4] . copy_from_slice (& (1i32 as i32) . to_le_bytes ())")
        );
        assert!(rendered.contains("InvalidWireValue :: EnumTag"));
        assert!(rendered.contains("let (__boltffi_payload0_decoded , __boltffi_payload0_used)"));
        assert!(rendered.contains("let (__boltffi_name_decoded , __boltffi_name_used)"));
    }

    #[test]
    fn native_data_enum_expansion_emits_encoded_instance_method_wrapper() {
        let method = record_method(
            "label",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::String),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(event_enum_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_enum_demo_event_label"));
        assert!(rendered.contains("__boltffi_receiver_ptr : * const u8"));
        assert!(rendered.contains("__boltffi_receiver_len : usize"));
        assert!(rendered.contains("let __boltffi_receiver : Event ="));
        assert!(!rendered.contains("__boltffi_receiver_storage"));
        assert!(rendered.contains("__boltffi_receiver . label ()"));
    }

    #[test]
    fn native_data_enum_expansion_emits_async_instance_method_wrapper() {
        let mut method = record_method(
            "label",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::String),
        );
        method.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(event_enum_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        let generated = quote! {
            pub enum Event {
                Empty,
                Count(u32),
                Named { name: String },
            }

            impl Event {
                pub async fn label(&self) -> String {
                    String::new()
                }
            }

            #tokens
        };
        syn::parse2::<syn::File>(generated.clone())
            .expect("data enum async method expansion parses");
        assert_generated_crate_checks("native_data_enum_async_method", generated);
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_enum_demo_event_label"));
        assert!(rendered.contains("fn boltffi_async_method_enum_demo_event_label_poll"));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: rustfuture :: rust_future_new (async move")
        );
    }

    #[test]
    fn wasm_data_enum_expansion_emits_async_instance_method_wrapper() {
        let mut method = record_method(
            "label",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::String),
        );
        method.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(event_enum_with_method(method));
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Event {
                Empty,
                Count(u32),
                Named { name: String },
            }

            impl Event {
                pub async fn label(&self) -> String {
                    String::new()
                }
            }

            #tokens
        })
        .expect("wasm data enum async method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_method_enum_demo_event_label"));
        assert!(rendered.contains("__boltffi_receiver_ptr : * const u8"));
        assert!(rendered.contains("__boltffi_receiver_len : usize"));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: rustfuture :: rust_future_new (async move")
        );
        assert!(rendered.contains("fn boltffi_async_method_enum_demo_event_label_poll_sync"));
    }

    #[test]
    fn native_data_enum_expansion_emits_static_method_wrapper() {
        let method = record_method(
            "kind_count",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(event_enum_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Event {
                Empty,
                Count(u32),
                Named { name: String },
            }

            impl Event {
                pub fn kind_count() -> u32 {
                    3
                }
            }

            #tokens
        })
        .expect("data enum static method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_enum_demo_event_kind_count"));
        assert!(!rendered.contains("fn boltffi_init_enum_demo_event_kind_count"));
        assert!(!rendered.contains("__boltffi_receiver"));
        assert!(rendered.contains("Event :: kind_count ()"));
    }

    #[test]
    fn native_data_enum_expansion_emits_initializer_wrapper() {
        let initializer = record_method(
            "empty_event",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(event_enum_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Event {
                Empty,
                Count(u32),
                Named { name: String },
            }

            impl Event {
                pub fn empty_event() -> Self {
                    Self::Empty
                }
            }

            #tokens
        })
        .expect("data enum initializer expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_enum_demo_event_empty_event"));
        assert!(!rendered.contains("fn boltffi_method_enum_demo_event_empty_event"));
        assert!(rendered.contains("Event :: empty_event ()"));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: wire_encode"));
    }

    #[test]
    fn wasm_data_enum_expansion_emits_initializer_wrapper() {
        let initializer = record_method(
            "empty_event",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(event_enum_with_method(initializer));
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        let rendered = tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_init_enum_demo_event_empty_event"));
        assert!(rendered.contains("Event :: empty_event ()"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_result) . into_packed ()"
        ));
    }

    #[test]
    fn native_data_enum_expansion_emits_async_initializer_wrapper() {
        let mut initializer = record_method(
            "load",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        initializer.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.enums.push(event_enum_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");

        syn::parse2::<syn::File>(quote! {
            pub enum Event {
                Empty,
                Count(u32),
                Named { name: String },
            }

            impl Event {
                pub async fn load() -> Self {
                    Self::Empty
                }
            }

            #tokens
        })
        .expect("data enum async initializer expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_enum_demo_event_load"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: rustfuture :: rust_future_new (async move { Event :: load () . await })"
        ));
        assert!(rendered.contains("fn boltffi_async_init_enum_demo_event_load_poll"));
        assert!(rendered.contains("fn boltffi_async_init_enum_demo_event_load_complete"));
    }

    #[test]
    fn native_custom_return_expansion_calls_into_ffi_before_wire_encode() {
        let source = custom_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn stamp() -> Timestamp {
                Timestamp::now()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn stamp() -> Timestamp {
                    Timestamp::now()
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_stamp() -> ::boltffi::__private::FfiBuf {
                    let __boltffi_result = stamp();
                    {
                        let __boltffi_wire = (timestamp_into_ffi)(&__boltffi_result);
                        ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_wire)
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_custom_error_expansion_converts_error_before_wire_encode() {
        let source = custom_result_error_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_stamp() -> Result<u32, Timestamp> {
                Err(Timestamp::now())
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_function_demo_try_stamp (__boltffi_return_out : * mut u32) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains("Err (__boltffi_error) =>"));
        assert!(
            rendered.contains("let __boltffi_wire = (timestamp_into_ffi) (& __boltffi_error) ;")
        );
        assert!(
            rendered
                .contains(":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_wire)")
        );
    }

    #[test]
    fn native_nested_custom_return_expansion_converts_every_inner_value() {
        let source = nested_custom_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn timeline() -> Vec<Option<Timestamp>> {
                Vec::new()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();

        assert!(rendered.contains("let __boltffi_result = timeline () ;"));
        assert!(rendered.contains(". into_iter () . map (| value | value . map (| value | (timestamp_into_ffi) (& value))) . collect :: < Vec < _ >> ()"));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_wire)")
        );
    }

    #[test]
    fn function_expansion_uses_restricted_source_visibility() {
        let source = source_visibility_contract(Visibility::Restricted("crate".to_owned()));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub(in crate) extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_function_expansion_uses_wasm_cfg() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn function_wrappers_compose_native_and_wasm_without_reemitting_source_item() {
        let source = source_contract();
        let native_lowered =
            lower_with_declarations::<Native>(&source).expect("native lowered bindings");
        let wasm_lowered =
            lower_with_declarations::<Wasm32>(&source).expect("wasm lowered bindings");
        let native_expansion = Expansion::new(&native_lowered);
        let wasm_expansion = Expansion::new(&wasm_lowered);
        let syntax: ItemFn = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let native_wrapper = wrapper::function::Renderer::new(
            native_expansion.function(&source.functions[0]).unwrap(),
            &native_expansion,
        )
        .render()
        .expect("native wrapper");
        let wasm_wrapper = wrapper::function::Renderer::new(
            wasm_expansion.function(&source.functions[0]).unwrap(),
            &wasm_expansion,
        )
        .render()
        .expect("wasm wrapper");

        let tokens = quote! {
            #syntax
            #native_wrapper
            #wasm_wrapper
        };

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_async_function_expansion_exports_poll_handle_lifecycle() {
        let source = async_answer_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn answer() -> u32 {
                42
            }
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> ::boltffi::__private::RustFutureHandle {
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        answer().await
                    })
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_poll(
                    handle: ::boltffi::__private::RustFutureHandle,
                    callback_data: u64,
                    callback: ::boltffi::__private::RustFutureContinuationCallback,
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_poll::<u32>(
                            handle,
                            callback,
                            callback_data
                        )
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                ) -> u32 {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<u32>(handle) } {
                        Ok(result) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = ::boltffi::__private::FfiStatus::OK;
                                }
                            }
                            result
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = status;
                                }
                            }
                            <u32 as ::core::default::Default>::default()
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_panic_message::<u32>(handle) } {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_cancel::<u32>(handle)
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_free::<u32>(handle)
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_async_encoded_return_expansion_completes_with_encoded_value() {
        let source = async_greet_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn greet() -> String {
                String::from("hello")
            }
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn greet() -> String {
                    String::from("hello")
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_greet() -> ::boltffi::__private::RustFutureHandle {
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        greet().await
                    })
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_poll(
                    handle: ::boltffi::__private::RustFutureHandle,
                    callback_data: u64,
                    callback: ::boltffi::__private::RustFutureContinuationCallback,
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_poll::<String>(
                            handle,
                            callback,
                            callback_data
                        )
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                ) -> ::boltffi::__private::FfiBuf {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<String>(handle) } {
                        Ok(__boltffi_result) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = ::boltffi::__private::FfiStatus::OK;
                                }
                            }
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result)
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = status;
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_panic_message::<String>(handle) } {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_cancel::<String>(handle)
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_free::<String>(handle)
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_async_result_expansion_writes_success_out_pointer_and_encoded_error() {
        let source = async_result_i32_string_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn try_count() -> Result<i32, String> {
                Ok(7)
            }
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");
        let rust_return_type: syn::Type = syn::parse_quote! { Result<i32, String> };

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn try_count() -> Result<i32, String> {
                    Ok(7)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_try_count() -> ::boltffi::__private::RustFutureHandle {
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        try_count().await
                    })
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_poll(
                    handle: ::boltffi::__private::RustFutureHandle,
                    callback_data: u64,
                    callback: ::boltffi::__private::RustFutureContinuationCallback,
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_poll::<#rust_return_type>(
                            handle,
                            callback,
                            callback_data
                        )
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                    __boltffi_return_out: *mut i32
                ) -> ::boltffi::__private::FfiBuf {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) } {
                        Ok(Ok(__boltffi_success)) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = ::boltffi::__private::FfiStatus::OK;
                                }
                            }
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    ::core::ptr::write(
                                        __boltffi_return_out,
                                        __boltffi_success
                                    );
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                        Ok(Err(__boltffi_error)) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = ::boltffi::__private::FfiStatus::OK;
                                }
                            }
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error)
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = status;
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_panic_message::<#rust_return_type>(handle) } {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle)
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle)
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_async_void_function_expansion_exports_sync_poll_lifecycle() {
        let source = async_ping_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn ping() {}
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn ping() {}
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_ping() -> ::boltffi::__private::RustFutureHandle {
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        ping().await
                    })
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_poll_sync(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> i32 {
                    unsafe {
                        ::boltffi::__private::rust_future_poll_sync::<()>(handle)
                    }
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                ) {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<()>(handle) } {
                        Ok(_) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = ::boltffi::__private::FfiStatus::OK;
                                }
                            }
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = status;
                                }
                            }
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_panic_message::<()>(handle) } {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_cancel::<()>(handle)
                    }
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_free::<()>(handle)
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_async_owned_string_param_expansion_decodes_before_spawning_future() {
        let source = async_string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn name_len(name: String) -> u32 {
                name.len() as u32
            }
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn name_len(name: String) -> u32 {
                    name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_name_len(
                    __boltffi_name_ptr: *const u8,
                    __boltffi_name_len: usize
                ) -> ::boltffi::__private::RustFutureHandle {
                    let name: String = {
                        if __boltffi_name_ptr.is_null() && __boltffi_name_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(name),
                                __boltffi_name_len
                            ));
                            return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<u32>();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_name_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_name_ptr,
                                    __boltffi_name_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<String>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(name),
                                    error,
                                    __boltffi_name_len
                                ));
                                return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<u32>();
                            }
                        }
                    };
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        name_len(name).await
                    })
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_poll(
                    handle: ::boltffi::__private::RustFutureHandle,
                    callback_data: u64,
                    callback: ::boltffi::__private::RustFutureContinuationCallback,
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_poll::<u32>(
                            handle,
                            callback,
                            callback_data
                        )
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                ) -> u32 {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<u32>(handle) } {
                        Ok(result) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = ::boltffi::__private::FfiStatus::OK;
                                }
                            }
                            result
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                unsafe {
                                    *out_status = status;
                                }
                            }
                            <u32 as ::core::default::Default>::default()
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match unsafe { ::boltffi::__private::rustfuture::rust_future_panic_message::<u32>(handle) } {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_cancel::<u32>(handle)
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    unsafe {
                        ::boltffi::__private::rustfuture::rust_future_free::<u32>(handle)
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn async_borrowed_param_expansion_rejects_reference_capture() {
        let source = async_borrowed_string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn name_len(name: &str) -> u32 {
                name.len() as u32
            }
        };

        let error = expand_function(&expansion, &source.functions[0], syntax)
            .expect_err("async borrowed params must not capture FFI memory");

        assert_eq!(
            error.to_string(),
            "unsupported expansion: async reference parameter"
        );
    }

    #[test]
    fn void_function_expansion_returns_status() {
        let source = void_source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn ping() {}
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn ping() {}
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_ping() -> ::boltffi::__private::FfiStatus {
                    ping();
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn direct_record_param_expansion_unpacks_passable_input() {
        let source = direct_record_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn norm(point: Point) -> f64 {
                point.x
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn norm(point: Point) -> f64 {
                    point.x
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_norm(
                    point: <Point as ::boltffi::__private::Passable>::In
                ) -> f64 {
                    let point: Point = unsafe {
                        <Point as ::boltffi::__private::Passable>::unpack(point)
                    };
                    norm(point)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_direct_record_param_expansion_reads_writer_pointer() {
        let source = direct_record_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn norm(point: Point) -> f64 {
                point.x
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn norm(point: Point) -> f64 {
                    point.x
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_norm(
                    point: *const u8
                ) -> f64 {
                    if point.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null direct record pointer",
                            stringify!(point)
                        ));
                        return <f64 as ::core::default::Default>::default();
                    }
                    let point: Point = unsafe {
                        let __boltffi_value =
                            ::core::ptr::read_unaligned(
                                point as *const <Point as ::boltffi::__private::Passable>::In
                            );
                        <Point as ::boltffi::__private::Passable>::unpack(__boltffi_value)
                    };
                    norm(point)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn mutable_direct_record_param_expansion_passes_mutable_local() {
        let source = mutable_direct_record_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn shift(point: &mut Point) -> f64 {
                point.x += 1.0;
                point.x
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn shift(point: &mut Point) -> f64 {
                    point.x += 1.0;
                    point.x
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_shift(
                    point: *mut <Point as ::boltffi::__private::Passable>::In
                ) -> f64 {
                    if point.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null direct record pointer",
                            stringify!(point)
                        ));
                        return <f64 as ::core::default::Default>::default();
                    }
                    let point: &mut Point = unsafe { &mut *(point as *mut Point) };
                    shift(point)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_mutable_direct_record_param_expansion_writes_mutated_value_back() {
        let source = mutable_direct_record_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn shift(point: &mut Point) -> f64 {
                point.x += 1.0;
                point.x
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn shift(point: &mut Point) -> f64 {
                    point.x += 1.0;
                    point.x
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_shift(
                    point: *mut u8
                ) -> f64 {
                    let __boltffi_point_out = point;
                    if __boltffi_point_out.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null direct record pointer",
                            stringify!(point)
                        ));
                        return <f64 as ::core::default::Default>::default();
                    }
                    let mut point: Point = unsafe {
                        let __boltffi_value =
                            ::core::ptr::read_unaligned(
                                __boltffi_point_out as *const <Point as ::boltffi::__private::Passable>::In
                            );
                        <Point as ::boltffi::__private::Passable>::unpack(__boltffi_value)
                    };
                    let __boltffi_result = shift(&mut point);
                    unsafe {
                        ::core::ptr::write_unaligned(
                            __boltffi_point_out as *mut <Point as ::boltffi::__private::Passable>::In,
                            <Point as ::boltffi::__private::Passable>::pack(point)
                        );
                    }
                    __boltffi_result
                }
            }
            .to_string()
        );
    }

    #[test]
    fn mutable_direct_primitive_param_expansion_passes_mutable_local() {
        let source = mutable_direct_primitive_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn bump(count: &mut i32) {
                *count += 1;
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn bump(count: &mut i32) {
                    *count += 1;
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_bump(
                    count: i32
                ) -> ::boltffi::__private::FfiStatus {
                    let mut count = count;
                    bump(&mut count);
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_string_param_expansion_decodes_owned_string() {
        let source = string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn name_len(name: String) -> u32 {
                name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn name_len(name: String) -> u32 {
                    name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_name_len(
                    __boltffi_name_ptr: *const u8,
                    __boltffi_name_len: usize
                ) -> u32 {
                    let name: String = {
                        if __boltffi_name_ptr.is_null() && __boltffi_name_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(name),
                                __boltffi_name_len
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_name_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_name_ptr,
                                    __boltffi_name_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<String>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(name),
                                    error,
                                    __boltffi_name_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        }
                    };
                    name_len(name)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_borrowed_string_param_expansion_decodes_str_ref() {
        let source = borrowed_string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn name_len(name: &str) -> u32 {
                name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn name_len(name: &str) -> u32 {
                    name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_name_len(
                    __boltffi_name_ptr: *const u8,
                    __boltffi_name_len: usize
                ) -> u32 {
                    let __boltffi_name_storage: String = {
                        if __boltffi_name_ptr.is_null() && __boltffi_name_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(__boltffi_name_storage),
                                __boltffi_name_len
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_name_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_name_ptr,
                                    __boltffi_name_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<String>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(__boltffi_name_storage),
                                    error,
                                    __boltffi_name_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        }
                    };
                    let name = __boltffi_name_storage.as_str();
                    name_len(name)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_mutable_string_param_expansion_decodes_mut_str_ref() {
        let source = mutable_string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn rewrite(name: &mut str) -> u32 {
                name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn rewrite(name: &mut str) -> u32 {
                    name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_rewrite(
                    __boltffi_name_ptr: *const u8,
                    __boltffi_name_len: usize,
                    __boltffi_name_out: *mut ::boltffi::__private::FfiBuf
                ) -> u32 {
                    let mut __boltffi_name_storage: String = {
                        if __boltffi_name_ptr.is_null() && __boltffi_name_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(__boltffi_name_storage),
                                __boltffi_name_len
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_name_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_name_ptr,
                                    __boltffi_name_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<String>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(__boltffi_name_storage),
                                    error,
                                    __boltffi_name_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        }
                    };
                    let name = __boltffi_name_storage.as_mut_str();
                    if __boltffi_name_out.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: writeback pointer is null",
                            stringify!(__boltffi_name_out)
                        ));
                        return <u32 as ::core::default::Default>::default();
                    }
                    let __boltffi_result = rewrite(name);
                    unsafe {
                        ::core::ptr::write(
                            __boltffi_name_out,
                            ::boltffi::__private::FfiBuf::wire_encode(
                                &__boltffi_name_storage
                            )
                        );
                    }
                    __boltffi_result
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_bytes_param_expansion_decodes_owned_bytes() {
        let source = bytes_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn bytes_len(bytes: Vec<u8>) -> u32 {
                bytes.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn bytes_len(bytes: Vec<u8>) -> u32 {
                    bytes.len() as u32
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_bytes_len(
                    __boltffi_bytes_ptr: *const u8,
                    __boltffi_bytes_len: usize
                ) -> u32 {
                    let bytes: Vec<u8> = {
                        if __boltffi_bytes_ptr.is_null() && __boltffi_bytes_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(bytes),
                                __boltffi_bytes_len
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_bytes_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_bytes_ptr,
                                    __boltffi_bytes_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<Vec<u8> >(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(bytes),
                                    error,
                                    __boltffi_bytes_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        }
                    };
                    bytes_len(bytes)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_mutable_bytes_param_expansion_decodes_mut_slice_ref() {
        let source = mutable_bytes_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn fill(bytes: &mut [u8]) -> u32 {
                bytes.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn fill(bytes: &mut [u8]) -> u32 {
                    bytes.len() as u32
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_fill(
                    __boltffi_bytes_ptr: *const u8,
                    __boltffi_bytes_len: usize
                ) -> u32 {
                    let mut __boltffi_bytes_storage: Vec<u8> = {
                        if __boltffi_bytes_ptr.is_null() && __boltffi_bytes_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(__boltffi_bytes_storage),
                                __boltffi_bytes_len
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_bytes_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_bytes_ptr,
                                    __boltffi_bytes_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<Vec<u8> >(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(__boltffi_bytes_storage),
                                    error,
                                    __boltffi_bytes_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        }
                    };
                    let bytes = __boltffi_bytes_storage.as_mut_slice();
                    fill(bytes)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_option_i32_param_expansion_decodes_wire_option() {
        let source = option_i32_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn set_count(count: Option<i32>) {}
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn set_count(count: Option<i32>) {}
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_set_count(
                    __boltffi_count_ptr: *const u8,
                    __boltffi_count_len: usize
                ) -> ::boltffi::__private::FfiStatus {
                    let count: Option<i32> = if __boltffi_count_ptr.is_null() {
                        None
                    } else {
                        match ::boltffi::__private::wire::decode::<Option<i32> >(unsafe {
                            ::core::slice::from_raw_parts(
                                __boltffi_count_ptr,
                                __boltffi_count_len
                            )
                        }) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: invalid optional scalar payload: {} (buf_len={})",
                                    stringify!(count),
                                    error,
                                    __boltffi_count_len
                                ));
                                return ::boltffi::__private::FfiStatus::INVALID_ARG;
                            }
                        }
                    };
                    set_count(count);
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_encoded_record_param_expansion_returns_on_decode_failure() {
        let source = encoded_record_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn name_score(profile: Profile) -> u32 {
                profile.name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn name_score(profile: Profile) -> u32 {
                    profile.name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_name_score(
                    __boltffi_profile_ptr: *const u8,
                    __boltffi_profile_len: usize
                ) -> u32 {
                    let profile: Profile = {
                        if __boltffi_profile_ptr.is_null() && __boltffi_profile_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(profile),
                                __boltffi_profile_len
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_profile_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_profile_ptr,
                                    __boltffi_profile_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<Profile>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(profile),
                                    error,
                                    __boltffi_profile_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        }
                    };
                    name_score(profile)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_mutable_encoded_record_param_expansion_passes_mutable_storage_ref() {
        let source = mutable_encoded_record_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn rename(profile: &mut Profile) -> u32 {
                profile.name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn rename(profile: &mut Profile) -> u32 {
                    profile.name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_rename(
                    __boltffi_profile_ptr: *const u8,
                    __boltffi_profile_len: usize,
                    __boltffi_profile_out: *mut ::boltffi::__private::FfiBuf
                ) -> u32 {
                    let mut __boltffi_profile_storage: Profile = {
                        if __boltffi_profile_ptr.is_null() && __boltffi_profile_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(__boltffi_profile_storage),
                                __boltffi_profile_len
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_profile_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_profile_ptr,
                                    __boltffi_profile_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<Profile>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(__boltffi_profile_storage),
                                    error,
                                    __boltffi_profile_len
                                ));
                                return <u32 as ::core::default::Default>::default();
                            }
                        }
                    };
                    let profile = &mut __boltffi_profile_storage;
                    if __boltffi_profile_out.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: writeback pointer is null",
                            stringify!(__boltffi_profile_out)
                        ));
                        return <u32 as ::core::default::Default>::default();
                    }
                    let __boltffi_result = rename(profile);
                    unsafe {
                        ::core::ptr::write(
                            __boltffi_profile_out,
                            ::boltffi::__private::FfiBuf::wire_encode(
                                &__boltffi_profile_storage
                            )
                        );
                    }
                    __boltffi_result
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_class_param_expansion_consumes_required_handle_and_returns_nullable_handle() {
        let source = class_param_nullable_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn open(engine: Engine) -> Option<Engine> {
                Some(engine)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn open(engine: Engine) -> Option<Engine> {
                    Some(engine)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_open(
                    engine: u64
                ) -> u64 {
                    if engine == 0 {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null class handle",
                            stringify!(engine)
                        ));
                        return 0;
                    }
                    let engine: Engine = match unsafe {
                        __BoltffiEngineHandle::take(engine as usize as *mut __BoltffiEngineHandle)
                    } {
                        Some(value) => value,
                        None => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: released class handle",
                                stringify!(engine)
                            ));
                            return 0;
                        }
                    };
                    let __boltffi_result: Option<Engine> = open(engine);
                    match __boltffi_result {
                        Some(__boltffi_value) => {
                            __BoltffiEngineHandle::new(__boltffi_value) as usize as u64
                        }
                        None => 0,
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_class_expansion_emits_release_function() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            #tokens
        })
        .expect("class release expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_release_class_demo_engine"));
        assert!(rendered.contains("handle : u64"));
        assert!(rendered.contains("if handle != 0"));
        assert!(rendered.contains(
            "__BoltffiEngineHandle :: release (handle as usize as * mut __BoltffiEngineHandle)"
        ));
    }

    #[test]
    fn native_class_stream_expansion_emits_direct_batch_protocol() {
        let source = engine_stream_contract(stream("values", TypeExpr::Primitive(Primitive::I32)));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub struct Engine {
                producer: StreamProducer<i32>,
            }

            impl Engine {
                pub fn values(&self) -> Arc<EventSubscription<i32>> {
                    self.producer.subscribe()
                }
            }

            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("stream expansion parses");
        assert_generated_crate_checks("native_direct_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("fn boltffi_stream_demo_engine_values_subscribe"));
        assert!(rendered.contains("fn boltffi_stream_demo_engine_values_pop_batch"));
        assert!(rendered.contains(") -> u64"));
        assert!(rendered.contains("subscription_handle : u64"));
        assert!(
            rendered.contains(
                "output_ptr : * mut < i32 as :: boltffi :: __private :: Passable > :: Out"
            )
        );
        assert!(rendered.contains("subscription . pop_batch_into (__boltffi_stream_output_slots)"));
        assert!(rendered.contains("Passable < Out = StreamItem >"));
        assert!(!rendered.contains("< i32 as :: boltffi :: __private :: Passable > :: pack"));
        assert!(rendered.contains("Arc :: from_raw"));
    }

    #[test]
    fn native_class_stream_expansion_emits_direct_record_batch_protocol() {
        let source = point_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let record_tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");
        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            #[repr(C)]
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }

            pub struct Engine {
                producer: StreamProducer<Point>,
            }

            impl Engine {
                pub fn points(&self) -> Arc<EventSubscription<Point>> {
                    self.producer.subscribe()
                }
            }

            #record_tokens
            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("direct record stream expansion parses");
        assert_generated_crate_checks("native_direct_record_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains(
            "output_ptr : * mut < Point as :: boltffi :: __private :: Passable > :: Out"
        ));
        assert!(rendered.contains("subscription_handle : u64"));
        assert!(rendered.contains("subscription . pop_batch_into (__boltffi_stream_output_slots)"));
        assert!(rendered.contains("Passable < Out = StreamItem >"));
        assert!(!rendered.contains("< Point as :: boltffi :: __private :: Passable > :: pack"));
    }

    #[test]
    fn native_class_stream_expansion_packs_direct_enum_items() {
        let source = status_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let enum_tokens = expand_enumeration(&expansion, &source.enums[0]).expect("expanded enum");
        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            #[repr(i32)]
            #[derive(Clone, Copy)]
            pub enum Status {
                Ready = 0,
                Failed = 1,
            }

            pub struct Engine {
                producer: StreamProducer<Status>,
            }

            impl Engine {
                pub fn statuses(&self) -> Arc<EventSubscription<Status>> {
                    self.producer.subscribe()
                }
            }

            #enum_tokens
            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("direct enum stream expansion parses");
        assert_generated_crate_checks("native_direct_enum_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("< Status as :: boltffi :: __private :: Passable > :: pack"));
        assert!(!rendered.contains("subscription . pop_batch_into"));
    }

    #[test]
    fn native_class_stream_expansion_emits_encoded_batch_protocol() {
        let source = profile_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let record_tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");
        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            #[derive(Clone)]
            pub struct Profile {
                pub name: String,
            }

            pub struct Engine {
                producer: StreamProducer<Profile>,
            }

            impl Engine {
                pub fn profiles(&self) -> Arc<EventSubscription<Profile>> {
                    self.producer.subscribe()
                }
            }

            #record_tokens
            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("encoded stream expansion parses");
        assert_generated_crate_checks("native_encoded_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("max_count : usize"));
        assert!(rendered.contains("-> :: boltffi :: __private :: FfiBuf"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_stream_items)"
        ));
    }

    #[test]
    fn native_class_stream_expansion_uses_owned_storage_for_str_items() {
        let source = engine_stream_contract(stream("lines", TypeExpr::Str));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub struct Engine {
                producer: StreamProducer<String>,
            }

            impl Engine {
                pub fn lines(&self) -> Arc<EventSubscription<String>> {
                    self.producer.subscribe()
                }
            }

            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("str stream expansion parses");
        assert_generated_crate_checks("native_str_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("EventSubscription < String >"));
    }

    #[test]
    fn native_class_stream_expansion_uses_owned_storage_for_slice_items() {
        let source = engine_stream_contract(stream(
            "byte_chunks",
            TypeExpr::slice(TypeExpr::Primitive(Primitive::U8)),
        ));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub struct Engine {
                producer: StreamProducer<Vec<u8>>,
            }

            impl Engine {
                pub fn byte_chunks(&self) -> Arc<EventSubscription<Vec<u8>>> {
                    self.producer.subscribe()
                }
            }

            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("slice stream expansion parses");
        assert_generated_crate_checks("native_slice_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("EventSubscription < Vec < u8 > >"));
    }

    #[test]
    fn native_class_stream_expansion_emits_tuple_batches() {
        let source = engine_stream_contract(stream(
            "points",
            TypeExpr::tuple(vec![TypeExpr::Primitive(Primitive::I32), TypeExpr::String]),
        ));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub struct Engine {
                producer: StreamProducer<(i32, String)>,
            }

            impl Engine {
                pub fn points(&self) -> Arc<EventSubscription<(i32, String)>> {
                    self.producer.subscribe()
                }
            }

            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("tuple stream expansion parses");
        assert_generated_crate_checks("native_tuple_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("FfiBuf :: wire_encode (& __boltffi_stream_items)"));
    }

    #[test]
    fn native_class_stream_expansion_emits_map_batches() {
        let source = engine_stream_contract(stream(
            "counts",
            TypeExpr::hash_map(TypeExpr::String, TypeExpr::Primitive(Primitive::U32)),
        ));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::collections::HashMap;
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub struct Engine {
                producer: StreamProducer<HashMap<String, u32>>,
            }

            impl Engine {
                pub fn counts(&self) -> Arc<EventSubscription<HashMap<String, u32>>> {
                    self.producer.subscribe()
                }
            }

            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("map stream expansion parses");
        assert_generated_crate_checks("native_map_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("FfiBuf :: wire_encode (& __boltffi_stream_items)"));
    }

    #[test]
    fn native_class_stream_expansion_converts_custom_items_before_encoding() {
        let source = timestamp_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            #[derive(Clone)]
            pub struct Timestamp(i64);

            pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                value.0
            }

            pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                Ok(Timestamp(value))
            }

            pub struct Engine {
                producer: StreamProducer<Timestamp>,
            }

            impl Engine {
                pub fn timestamps(&self) -> Arc<EventSubscription<Timestamp>> {
                    self.producer.subscribe()
                }
            }

            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("custom stream expansion parses");
        assert_generated_crate_checks("native_custom_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(
            rendered.contains(". into_iter () . map (| value | (timestamp_into_ffi) (& value))")
        );
        assert!(rendered.contains("FfiBuf :: wire_encode (& __boltffi_wire)"));
    }

    #[test]
    fn native_class_stream_expansion_converts_custom_tuple_items_before_encoding() {
        let mut source = engine_stream_contract(stream(
            "events",
            TypeExpr::tuple(vec![
                custom_timestamp(),
                TypeExpr::Primitive(Primitive::I32),
            ]),
        ));
        source.customs.push(timestamp_custom_def());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            #[derive(Clone)]
            pub struct Timestamp(i64);

            pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                value.0
            }

            pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                Ok(Timestamp(value))
            }

            pub struct Engine {
                producer: StreamProducer<(Timestamp, i32)>,
            }

            impl Engine {
                pub fn events(&self) -> Arc<EventSubscription<(Timestamp, i32)>> {
                    self.producer.subscribe()
                }
            }

            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("custom tuple stream expansion parses");
        assert_generated_crate_checks("native_custom_tuple_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("timestamp_into_ffi"));
        assert!(rendered.contains("FfiBuf :: wire_encode (& __boltffi_wire)"));
    }

    #[test]
    fn native_class_stream_expansion_converts_custom_map_items_before_encoding() {
        let mut source = engine_stream_contract(stream(
            "timeline",
            TypeExpr::hash_map(TypeExpr::String, custom_timestamp()),
        ));
        source.customs.push(timestamp_custom_def());
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::collections::HashMap;
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            #[derive(Clone)]
            pub struct Timestamp(i64);

            pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                value.0
            }

            pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                Ok(Timestamp(value))
            }

            pub struct Engine {
                producer: StreamProducer<HashMap<String, Timestamp>>,
            }

            impl Engine {
                pub fn timeline(&self) -> Arc<EventSubscription<HashMap<String, Timestamp>>> {
                    self.producer.subscribe()
                }
            }

            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("custom map stream expansion parses");
        assert_generated_crate_checks("native_custom_map_stream", generated);
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("timestamp_into_ffi"));
        assert!(rendered.contains("FfiBuf :: wire_encode (& __boltffi_wire)"));
    }

    #[test]
    fn wasm_class_stream_expansion_packs_encoded_batches() {
        let source = profile_stream_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let record_tokens = expand_record(&expansion, &source.records[0]).expect("expanded record");
        let class_tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let stream_tokens = expand_stream(&expansion, &source.streams[0], &source.classes[0])
            .expect("expanded stream");

        let generated = quote! {
            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            #[derive(Clone)]
            pub struct Profile {
                pub name: String,
            }

            pub struct Engine {
                producer: StreamProducer<Profile>,
            }

            impl Engine {
                pub fn profiles(&self) -> Arc<EventSubscription<Profile>> {
                    self.producer.subscribe()
                }
            }

            #record_tokens
            #class_tokens
            #stream_tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("wasm stream expansion parses");
        assert_generated_crate_checks_target(
            "wasm_encoded_stream",
            "wasm32-unknown-unknown",
            generated,
        );
        let rendered = stream_tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_stream_demo_engine_profiles_pop_batch"));
        assert!(rendered.contains("fn boltffi_stream_demo_engine_profiles_subscribe"));
        assert!(rendered.contains(") -> u32"));
        assert!(rendered.contains("subscription_handle : u32"));
        assert!(rendered.contains(") -> u64"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_stream_items)"
        ));
        assert!(rendered.contains("into_packed ()"));
    }

    #[test]
    fn native_class_expansion_emits_initializer_and_static_method_wrappers() {
        let initializer = record_method(
            "new",
            Receiver::None,
            vec![parameter("seed", TypeExpr::Primitive(Primitive::U64))],
            ReturnDef::value(TypeExpr::SelfType),
        );
        let static_method = record_method(
            "count",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        let mut class = engine_class_with_method(initializer);
        class.methods.push(static_method);
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(class);
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            impl Engine {
                pub fn new(seed: u64) -> Self {
                    Self
                }

                pub fn count() -> u32 {
                    1
                }
            }

            #tokens
        })
        .expect("class static exports parse");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_class_demo_engine_new"));
        assert!(rendered.contains("Engine :: new (seed)"));
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_count"));
        assert!(rendered.contains("Engine :: count ()"));
        assert!(rendered.contains("trait BoltFFIThreadSafe : Send + Sync"));
        assert!(!rendered.contains("fn boltffi_init_class_demo_engine_count"));
    }

    #[test]
    fn native_class_expansion_emits_instance_method_wrapper() {
        let method = record_method("start", Receiver::Shared, Vec::new(), ReturnDef::Void);
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            impl Engine {
                pub fn start(&self) {}
            }

            #tokens
        })
        .expect("class instance method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_start"));
        assert!(rendered.contains("__boltffi_receiver : u64"));
        assert!(rendered.contains("if __boltffi_receiver == 0"));
        assert!(rendered.contains("let __boltffi_receiver : & Engine = unsafe"));
        assert!(rendered.contains("__boltffi_receiver . start ()"));
        assert!(rendered.contains("trait BoltFFIThreadSafe : Send + Sync"));
        assert!(rendered.contains("_assert :: < Engine > ()"));
    }

    #[test]
    fn native_single_threaded_class_expansion_omits_thread_safety_assertion() {
        let method = record_method("start", Receiver::Mutable, Vec::new(), ReturnDef::Void);
        let mut class = engine_class_with_method(method);
        class.thread_safety = ClassThreadSafety::UnsafeSingleThreaded;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(class);
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            impl Engine {
                pub fn start(&mut self) {}
            }

            #tokens
        })
        .expect("single-threaded class instance method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_start"));
        assert!(!rendered.contains("BoltFFIThreadSafe"));
    }

    #[test]
    fn native_class_method_returning_self_renders_concrete_class_type() {
        let method = record_method(
            "clone_engine",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            impl Engine {
                pub fn clone_engine(&self) -> Self {
                    Self
                }
            }

            #tokens
        })
        .expect("class self-returning method expansion parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_clone_engine"));
        assert!(rendered.contains("let __boltffi_result : Engine ="));
        assert!(!rendered.contains("let __boltffi_result : Self"));
        assert!(rendered.contains("__BoltffiEngineHandle :: new (__boltffi_result)"));
    }

    #[test]
    fn native_class_expansion_emits_async_instance_method_wrapper() {
        let mut method = record_method(
            "compute",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        method.execution = ExecutionKind::Async;
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        let generated = quote! {
            pub struct Engine;

            impl Engine {
                pub async fn compute(&self) -> u32 {
                    7
                }
            }

            #tokens
        };
        syn::parse2::<syn::File>(generated.clone()).expect("class async method expansion parses");
        assert_generated_crate_checks("native_class_async_method", generated);
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_compute"));
        assert!(rendered.contains("fn boltffi_async_method_class_demo_engine_compute_poll"));
        assert!(rendered.contains("rust_future_new (async move"));
        assert!(rendered.contains(
            "let __boltffi_receiver_handle = match unsafe { __BoltffiEngineHandle :: retain"
        ));
        assert!(
            rendered.contains(
                "let __boltffi_receiver : & Engine = __boltffi_receiver_handle . shared ()"
            )
        );
        assert!(rendered.contains("__boltffi_receiver . compute ()"));
    }

    #[test]
    fn wasm_class_expansion_uses_wasm_handle_carrier() {
        let method = record_method(
            "id",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        let rendered = tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_release_class_demo_engine (handle : u32)"));
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_id"));
        assert!(rendered.contains("__boltffi_receiver : u32"));
        assert!(rendered.contains("__boltffi_receiver . id ()"));
    }

    #[test]
    fn native_class_initializer_with_closure_param_expands() {
        let initializer = record_method(
            "new",
            Receiver::None,
            vec![parameter(
                "build",
                impl_closure(
                    vec![TypeExpr::Primitive(Primitive::U32)],
                    ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
                ),
            )],
            ReturnDef::value(TypeExpr::SelfType),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            impl Engine {
                pub fn new(build: impl Fn(u32) -> u32) -> Self {
                    let _ = build(1);
                    Self
                }
            }

            #tokens
        })
        .expect("class closure-param initializer parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_class_demo_engine_new"));
        assert!(rendered.contains("NativeCallbackOwner :: new"));
        assert!(rendered.contains("Engine :: new (build)"));
        assert_generated_crate_checks(
            "native_class_initializer_with_closure_param",
            quote! {
                pub struct Engine;

                impl Engine {
                    pub fn new(build: impl Fn(u32) -> u32) -> Self {
                        let _ = build(1);
                        Self
                    }
                }

                #tokens
            },
        );
    }

    #[test]
    fn native_class_method_returning_closure_expands() {
        let method = record_method(
            "handler",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(impl_closure(
                vec![TypeExpr::Primitive(Primitive::U32)],
                ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
            )),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            impl Engine {
                pub fn handler(&self) -> impl Fn(u32) -> u32 {
                    |value| value + 1
                }
            }

            #tokens
        })
        .expect("class closure-return method parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_handler"));
        assert!(rendered.contains("__boltffi_return_out : * mut :: core :: ffi :: c_void"));
        assert!(rendered.contains("__boltffi_handler_closure_call"));
        assert!(rendered.contains("__boltffi_receiver . handler ()"));
    }

    #[test]
    fn native_class_method_with_callback_param_and_return_expands() {
        let method = record_method(
            "replace",
            Receiver::Shared,
            vec![parameter("listener", boxed_listener())],
            ReturnDef::value(boxed_listener()),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait_with_method());
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");
        let callback_tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");

        syn::parse2::<syn::File>(quote! {
            pub trait Listener {
                fn on_value(&self, value: u32) -> u32;
            }

            pub struct Engine;

            impl Engine {
                pub fn replace(&self, listener: Box<dyn Listener>) -> Box<dyn Listener> {
                    listener
                }
            }

            #callback_tokens
            #tokens
        })
        .expect("class callback handle method with callback protocol parses");
        let rendered = quote! {
            #callback_tokens
            #tokens
        }
        .to_string();
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_replace"));
        assert!(rendered.contains("BoxFromCallbackHandle"));
        assert!(rendered.contains("pub struct ListenerVTable"));
        assert!(
            rendered.contains("pub unsafe extern \"C\" fn boltffi_register_callback_demo_listener")
        );
        assert!(rendered.contains("__boltffi_local_demo_listener_handle"));
        assert!(rendered.contains("__boltffi_receiver . replace (listener)"));
        assert_generated_crate_checks(
            "native_class_method_with_callback_param_and_return",
            quote! {
                pub trait Listener {
                    fn on_value(&self, value: u32) -> u32;
                }

                pub struct Engine;

                impl Engine {
                    pub fn replace(&self, listener: Box<dyn Listener>) -> Box<dyn Listener> {
                        listener
                    }
                }

                #callback_tokens
                #tokens
            },
        );
    }

    #[test]
    fn native_class_method_with_custom_param_and_return_expands() {
        let method = record_method(
            "stamp",
            Receiver::Shared,
            vec![parameter("when", custom_timestamp())],
            ReturnDef::value(custom_timestamp()),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Timestamp(i64);

            pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                value.0
            }

            pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                Ok(Timestamp(value))
            }

            pub struct Engine;

            impl Engine {
                pub fn stamp(&self, when: Timestamp) -> Timestamp {
                    when
                }
            }

            #tokens
        })
        .expect("class custom method parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_stamp"));
        assert!(rendered.contains("timestamp_try_from_ffi"));
        assert!(
            rendered.contains("let __boltffi_wire = (timestamp_into_ffi) (& __boltffi_result)")
        );
        assert!(rendered.contains("__boltffi_receiver . stamp (when)"));
        assert_generated_crate_checks(
            "native_class_method_with_custom_param_and_return",
            quote! {
                pub struct Timestamp(i64);

                pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                    value.0
                }

                pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                    Ok(Timestamp(value))
                }

                pub struct Engine;

                impl Engine {
                    pub fn stamp(&self, when: Timestamp) -> Timestamp {
                        when
                    }
                }

                #tokens
            },
        );
    }

    #[test]
    fn wasm_class_method_with_custom_param_and_return_expands() {
        let method = record_method(
            "stamp",
            Receiver::Shared,
            vec![parameter("when", custom_timestamp())],
            ReturnDef::value(custom_timestamp()),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.customs.push(timestamp_custom_def());
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Timestamp(i64);

            pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                value.0
            }

            pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                Ok(Timestamp(value))
            }

            pub struct Engine;

            impl Engine {
                pub fn stamp(&self, when: Timestamp) -> Timestamp {
                    when
                }
            }

            #tokens
        })
        .expect("wasm class custom method parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_stamp"));
        assert!(rendered.contains("__boltffi_receiver : u32"));
        assert!(rendered.contains("timestamp_try_from_ffi"));
        assert!(
            rendered.contains("let __boltffi_wire = (timestamp_into_ffi) (& __boltffi_result)")
        );
        assert!(rendered.contains("__boltffi_receiver . stamp (when)"));
    }

    #[test]
    fn native_class_result_self_initializer_expands() {
        let initializer = record_method(
            "try_new",
            Receiver::None,
            Vec::new(),
            result_return(TypeExpr::SelfType, TypeExpr::String),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class_with_method(initializer));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            impl Engine {
                pub fn try_new() -> Result<Self, String> {
                    Ok(Self)
                }
            }

            #tokens
        })
        .expect("class fallible initializer parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_init_class_demo_engine_try_new"));
        assert!(rendered.contains("__boltffi_return_out : * mut u64"));
        assert!(rendered.contains("Engine :: try_new ()"));
        assert!(rendered.contains(
            ":: core :: ptr :: write (__boltffi_return_out , __BoltffiEngineHandle :: new"
        ));
    }

    #[test]
    fn native_class_method_returning_nullable_self_expands() {
        let method = record_method(
            "maybe_self",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::option(TypeExpr::SelfType)),
        );
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class_with_method(method));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens = expand_class(&expansion, &source.classes[0]).expect("expanded class");

        syn::parse2::<syn::File>(quote! {
            pub struct Engine;

            impl Engine {
                pub fn maybe_self(&self) -> Option<Self> {
                    None
                }
            }

            #tokens
        })
        .expect("class nullable self-return method parses");
        let rendered = tokens.to_string();
        assert!(rendered.contains("fn boltffi_method_class_demo_engine_maybe_self"));
        assert!(rendered.contains("let __boltffi_result : Option < Engine >"));
        assert!(rendered.contains("Some (__boltffi_value)"));
        assert!(rendered.contains("None => 0"));
    }

    #[test]
    fn native_boxed_callback_param_expansion_recovers_boxed_trait_object() {
        let source = boxed_callback_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn listen(listener: Box<dyn Listener>) {}
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn listen(listener: Box<dyn Listener>) {}
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_listen(
                    listener: ::boltffi::__private::CallbackHandle
                ) -> ::boltffi::__private::FfiStatus {
                    let __boltffi_listener_handle = listener;
                    if __boltffi_listener_handle.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null callback handle",
                            stringify!(listener)
                        ));
                        return ::boltffi::__private::FfiStatus::INVALID_ARG;
                    }
                    let listener: Box<dyn Listener> = unsafe {
                        <ForeignListener as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(
                            __boltffi_listener_handle
                        )
                    };
                    listen(listener);
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_bounded_callback_param_expansion_recovers_foreign_proxy() {
        let source = boxed_send_callback_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn listen(listener: Box<dyn Listener + Send>) {
                drop(listener);
            }
        };

        let function_tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let callback_tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = function_tokens.to_string();

        assert!(rendered.contains("let listener : Box < dyn Listener + Send > = unsafe"));
        assert!(rendered.contains("< ForeignListener as :: boltffi :: __private :: BoxFromCallbackHandle > :: box_from_callback_handle"));
        assert_generated_crate_checks(
            "native_bounded_callback_param",
            quote! {
                pub trait Listener {}

                #callback_tokens
                #function_tokens
            },
        );
    }

    #[test]
    fn native_impl_trait_callback_param_expansion_recovers_foreign_proxy() {
        let source = impl_callback_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn listen(listener: impl Listener) {
                let _ = listener.on_value(1);
            }
        };

        let function_tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let callback_tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = function_tokens.to_string();

        assert!(rendered.contains("listener : :: boltffi :: __private :: CallbackHandle"));
        assert!(rendered.contains("let listener : ForeignListener = unsafe"));
        assert!(rendered.contains("< ForeignListener as :: boltffi :: __private :: BoxFromCallbackHandle > :: box_from_callback_handle"));
        assert!(rendered.contains("listen (listener)"));
        assert_generated_crate_checks(
            "native_impl_trait_callback_param",
            quote! {
                pub trait Listener {
                    fn on_value(&self, value: u32) -> u32;
                }

                #callback_tokens
                #function_tokens
            },
        );
    }

    #[test]
    fn wasm_impl_trait_callback_param_expansion_recovers_foreign_proxy() {
        let source = impl_callback_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn listen(listener: impl Listener) {
                let _ = listener.on_value(1);
            }
        };

        let function_tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let callback_tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = function_tokens.to_string();

        assert!(rendered.contains("listener : u32"));
        assert!(rendered.contains(":: boltffi :: __private :: CallbackHandle :: from_wasm_handle"));
        assert!(rendered.contains("let listener : ForeignListener = unsafe"));
        assert!(rendered.contains("< ForeignListener as :: boltffi :: __private :: BoxFromCallbackHandle > :: box_from_callback_handle"));
        assert_generated_crate_checks(
            "wasm_impl_trait_callback_param",
            quote! {
                pub trait Listener {
                    fn on_value(&self, value: u32) -> u32;
                }

                #callback_tokens
                #function_tokens
            },
        );
    }

    #[test]
    fn native_async_impl_trait_callback_param_expansion_captures_foreign_proxy() {
        let source = async_impl_callback_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn listen(listener: impl Listener) -> u32 {
                listener.on_value(1)
            }
        };

        let function_tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");
        let callback_tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = function_tokens.to_string();

        assert!(rendered.contains("listener : :: boltffi :: __private :: CallbackHandle"));
        assert!(rendered.contains("let listener : ForeignListener = unsafe"));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: rustfuture :: rust_future_new (async move")
        );
        assert!(rendered.contains("listen (listener) . await"));
        assert_generated_crate_checks(
            "native_async_impl_trait_callback_param",
            quote! {
                pub trait Listener {
                    fn on_value(&self, value: u32) -> u32;
                }

                #callback_tokens
                #function_tokens
            },
        );
    }

    #[test]
    fn wasm_async_impl_trait_callback_param_expansion_captures_foreign_proxy() {
        let source = async_impl_callback_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn listen(listener: impl Listener) -> u32 {
                listener.on_value(1)
            }
        };

        let function_tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");
        let callback_tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = function_tokens.to_string();

        assert!(rendered.contains("listener : u32"));
        assert!(rendered.contains(":: boltffi :: __private :: CallbackHandle :: from_wasm_handle"));
        assert!(rendered.contains("let listener : ForeignListener = unsafe"));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: rustfuture :: rust_future_new (async move")
        );
        assert!(rendered.contains("listen (listener) . await"));
        assert_generated_crate_checks(
            "wasm_async_impl_trait_callback_param",
            quote! {
                pub trait Listener {
                    fn on_value(&self, value: u32) -> u32;
                }

                #callback_tokens
                #function_tokens
            },
        );
    }

    #[test]
    fn native_callback_trait_expansion_emits_vtable_and_local_handle_protocol() {
        let source = listener_trait_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();

        assert!(rendered.contains("pub struct ListenerVTable"));
        assert!(rendered.contains("pub on_value : extern \"C\" fn (handle : u64 , u32) -> u32"));
        assert!(
            rendered.contains("pub unsafe extern \"C\" fn boltffi_register_callback_demo_listener")
        );
        assert!(rendered.contains("pub extern \"C\" fn boltffi_create_callback_demo_listener"));
        assert!(rendered.contains("impl Listener for ForeignListener"));
        assert!(
            rendered
                .contains("impl :: boltffi :: __private :: ArcFromCallbackHandle for dyn Listener")
        );
        assert!(rendered.contains("pub (crate) fn __boltffi_local_demo_listener_handle"));
        assert!(rendered.contains("static __BOLTFFI_LOCAL_LISTENER_VTABLE"));
    }

    #[test]
    fn wasm_callback_trait_expansion_emits_imports_and_local_registry_protocol() {
        let source = listener_trait_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();

        assert!(
            rendered.contains("fn __boltffi_callback_lifecycle_demo_listener_free (handle : u32)")
        );
        assert!(
            rendered.contains(
                "fn __boltffi_callback_lifecycle_demo_listener_clone (handle : u32) -> u32"
            )
        );
        assert!(rendered.contains(
            "fn __boltffi_callback_method_demo_listener_on_value (handle : u32 , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(rendered.contains("pub extern \"C\" fn boltffi_create_callback_demo_listener"));
        assert!(rendered.contains("impl Listener for ForeignListener"));
        assert!(rendered.contains("pub struct LocalListener"));
        assert!(rendered.contains("impl Listener for LocalListener"));
        assert!(rendered.contains("if handle < 2147483648u32"));
        assert!(rendered.contains("let handle = __boltffi_local_demo_listener_clone (handle)"));
        assert!(rendered.contains(":: std :: sync :: Arc :: new (LocalListener { handle })"));
        assert!(rendered.contains("Box :: new (LocalListener { handle })"));
        let local_drop = rendered
            .split("impl Drop for LocalListener")
            .nth(1)
            .and_then(|tail| tail.split("impl Listener for LocalListener").next())
            .expect("local listener drop body");
        assert!(local_drop.contains("__boltffi_local_demo_listener_free (self . handle)"));
        assert!(!local_drop.contains("__boltffi_callback_lifecycle_demo_listener_free"));
        assert!(rendered.contains("static __BOLTFFI_LOCAL_LISTENER_REGISTRY"));
        assert!(rendered.contains("pub extern \"C\" fn __boltffi_local_demo_listener_on_value"));
        assert!(rendered.contains("pub (crate) fn __boltffi_local_demo_listener_handle"));
    }

    #[test]
    fn wasm_callback_method_returning_bytes_uses_out_buffer() {
        let source = bytes_returning_listener_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "fn __boltffi_callback_method_demo_listener_payload (handle : u32 , __boltffi_callback_out : * mut :: boltffi :: __private :: WasmCallbackOutBuf)"
        ));
        assert!(rendered.contains(
            "let mut __boltffi_callback_out = :: boltffi :: __private :: WasmCallbackOutBuf :: empty ()"
        ));
        assert!(rendered.contains("__boltffi_callback_out . as_slice ()"));
        assert!(rendered.contains(
            "pub extern \"C\" fn __boltffi_local_demo_listener_payload (handle : u32) -> u64"
        ));
        assert!(rendered.contains(
            ":: boltffi :: __private :: take_packed_bytes (__boltffi_local_demo_listener_payload (self . handle))"
        ));
        assert!(!rendered.contains(
            "__boltffi_local_demo_listener_payload (self . handle , & mut __boltffi_callback_out)"
        ));
    }

    #[test]
    fn native_callback_method_direct_returns_use_return_renderers() {
        let source = direct_returning_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "pub maybe_count : extern \"C\" fn (handle : u64) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains(
            "pub numbers : extern \"C\" fn (handle : u64) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains("wire :: decode :: < Option < i32 > >"));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: wire_encode"));
        assert!(
            rendered.contains("< i32 as :: boltffi :: __private :: VecTransport > :: pack_vec")
        );
        assert!(
            rendered.contains("< i32 as :: boltffi :: __private :: VecTransport > :: unpack_vec")
        );
    }

    #[test]
    fn wasm_callback_method_direct_returns_use_return_renderers() {
        let source = direct_returning_listener_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "fn __boltffi_callback_method_demo_listener_maybe_count (handle : u32) -> f64"
        ));
        assert!(
            rendered.contains("fn __boltffi_callback_method_demo_listener_numbers (handle : u32)")
        );
        assert!(rendered.contains("f64 :: NAN"));
        assert!(rendered.contains(":: boltffi :: __private :: write_return_slot"));
        assert!(rendered.contains(":: boltffi :: __private :: take_return_slot_vec :: < i32 >"));
    }

    #[test]
    fn native_callback_method_borrowed_string_param_encodes_reference() {
        let source = borrowed_string_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains("fn on_name (& self , name : & str)"));
        assert!(
            rendered.contains("pub on_name : extern \"C\" fn (handle : u64 , * const u8 , usize)")
        );
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: wire_encode (& name)"));
    }

    #[test]
    fn native_callback_method_borrowed_direct_scalar_param_passes_value() {
        let source = borrowed_u32_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains("fn on_value (& self , value : & u32) -> u32"));
        assert!(rendered.contains("self . handle , * value"));
    }

    #[test]
    fn native_async_callback_method_expansion_uses_completion_callback() {
        let source = async_string_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "pub on_event : extern \"C\" fn (handle : u64 , i32 , completion : :: boltffi :: __private :: AsyncCallback < :: boltffi :: __private :: FfiBuf > , completion_data : * mut :: core :: ffi :: c_void)"
        ));
        assert!(rendered.contains("async fn on_event (& self , value : i32) -> String"));
        assert!(rendered.contains("__boltffi_completion_data"));
        assert!(rendered.contains("async callback return conversion failed"));
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: ArcFromCallbackHandle for dyn Listener")
        );
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: BoxFromCallbackHandle for dyn Listener")
        );
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: CallbackForeignType for dyn Listener")
        );
    }

    #[test]
    fn native_async_trait_callback_method_expansion_uses_async_trait_attribute() {
        let mut source = async_string_listener_contract();
        source.traits[0].user_attrs.push(UserAttr::new(
            Path::single("async_trait"),
            AttributeInput::Empty,
        ));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens).expect("expanded callback parses");

        assert!(
            rendered
                .contains("# [:: async_trait :: async_trait] impl Listener for ForeignListener")
        );
    }

    #[test]
    fn wasm_async_callback_method_expansion_uses_start_import_and_complete_export() {
        let source = async_string_listener_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "fn __boltffi_callback_async_start_demo_listener_on_event (handle : u32 , request_id : u32 , __boltffi_arg0 : i32)"
        ));
        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_callback_demo_listener_on_event_complete"
        ));
        assert!(rendered.contains("AsyncCallbackRegistry :: current"));
        assert!(rendered.contains("async callback return conversion failed"));
        assert!(!rendered.contains("pub struct LocalListener"));
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: ArcFromCallbackHandle for dyn Listener")
        );
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: BoxFromCallbackHandle for dyn Listener")
        );
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: CallbackForeignType for dyn Listener")
        );
    }

    #[test]
    fn native_async_callback_method_direct_returns_use_buffer_completion() {
        let source = async_direct_returning_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "completion : :: boltffi :: __private :: AsyncCallback < :: boltffi :: __private :: FfiBuf >"
        ));
        assert!(rendered.contains("completion : :: boltffi :: __private :: AsyncCallback < u32 >"));
        assert!(rendered.contains("wire :: decode :: < Option < i32 > >"));
        assert!(
            rendered.contains("< i32 as :: boltffi :: __private :: VecTransport > :: unpack_vec")
        );
    }

    #[test]
    fn wasm_async_callback_method_direct_returns_use_completion_bytes() {
        let source = async_direct_returning_listener_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "fn __boltffi_callback_async_start_demo_listener_maybe_count (handle : u32 , request_id : u32)"
        ));
        assert!(rendered.contains(
            "fn __boltffi_callback_async_start_demo_listener_count (handle : u32 , request_id : u32)"
        ));
        assert!(rendered.contains("wire :: decode :: < u32 >"));
        assert!(rendered.contains("wire :: decode :: < Option < i32 > >"));
        assert!(rendered.contains("__boltffi_completion . data . as_slice ()"));
        assert!(
            rendered.contains("< i32 as :: boltffi :: __private :: VecTransport > :: unpack_vec")
        );
        assert!(rendered.contains("__boltffi_completion . data . as_ptr ()"));
    }

    #[test]
    fn wasm_async_callback_method_callback_return_uses_completion_bytes() {
        let source = async_callback_returning_listener_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "fn __boltffi_callback_async_start_demo_listener_listener (handle : u32 , request_id : u32)"
        ));
        assert!(rendered.contains("wire :: decode :: < u32 >"));
        assert!(rendered.contains("CallbackHandle :: from_wasm_handle"));
        assert!(rendered.contains("< ForeignListener as :: boltffi :: __private :: BoxFromCallbackHandle > :: box_from_callback_handle"));
    }

    #[test]
    fn native_async_callback_method_result_return_decodes_success_and_error() {
        let source = async_fallible_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "completion : :: boltffi :: __private :: AsyncCallback < :: boltffi :: __private :: FfiBuf >"
        ));
        assert!(rendered.contains("async fn try_count (& self) -> Result < u32 , String >"));
        assert!(
            rendered.contains("async fn try_numbers (& self) -> Result < Vec < u8 > , String >")
        );
        assert!(rendered.contains("if state . status . is_err ()"));
        assert!(rendered.contains(":: boltffi :: __private :: wire :: decode :: < u32 >"));
        assert!(rendered.contains(":: boltffi :: __private :: wire :: decode :: < Vec < u8 > >"));
        assert!(rendered.contains(":: boltffi :: __private :: wire :: decode :: < String >"));
    }

    #[test]
    fn wasm_async_callback_method_result_return_decodes_completion_branch() {
        let source = async_fallible_listener_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "fn __boltffi_callback_async_start_demo_listener_try_count (handle : u32 , request_id : u32)"
        ));
        assert!(rendered.contains(
            "fn __boltffi_callback_async_start_demo_listener_try_numbers (handle : u32 , request_id : u32)"
        ));
        assert!(rendered.contains("async fn try_count (& self) -> Result < u32 , String >"));
        assert!(
            rendered.contains("async fn try_numbers (& self) -> Result < Vec < u8 > , String >")
        );
        assert!(rendered.contains("if __boltffi_completion . code . is_success ()"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < u32 > (__boltffi_completion . data . as_slice ())"
        ));
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < Vec < u8 > > (__boltffi_completion . data . as_slice ())"
        ));
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_completion . data . as_slice ())"
        ));
    }

    #[test]
    fn native_callback_method_closure_param_expansion_registers_closure() {
        let source = closure_taking_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains("fn on_render (& self , callback : impl Fn (u32) -> u32)"));
        assert!(rendered.contains("__boltffi_callback_call"));
        assert!(rendered.contains("__boltffi_callback_context"));
        assert!(rendered.contains("__boltffi_callback_release"));
        assert!(rendered.contains("pub on_render : extern \"C\" fn"));
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: ArcFromCallbackHandle for dyn Listener")
        );
        assert!(!rendered.contains("pub (crate) fn __boltffi_local_demo_listener_handle"));
    }

    #[test]
    fn native_callback_method_closure_param_expansion_encodes_closure_invocation() {
        let source = string_closure_taking_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains("fn on_render (& self , callback : impl Fn (String) -> String)"));
        assert!(rendered.contains(
            "extern \"C\" fn __boltffi_callback_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_arg0_ptr : * const u8 , __boltffi_arg0_len : usize) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains(":: boltffi :: __private :: wire :: decode :: < String >"));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: wire_encode"));
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: ArcFromCallbackHandle for dyn Listener")
        );
    }

    #[test]
    fn wasm_callback_method_closure_param_expansion_encodes_closure_invocation() {
        let source = string_closure_taking_listener_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains("fn on_render (& self , callback : impl Fn (String) -> String)"));
        assert!(rendered.contains("pub unsafe extern \"C\" fn boltffi_closure_"));
        assert!(rendered.contains(
            "(__boltffi_context : u32 , __boltffi_arg0_ptr : * const u8 , __boltffi_arg0_len : usize) -> u64"
        ));
        assert!(rendered.contains(":: boltffi :: __private :: wire :: decode :: < String >"));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: wire_encode"));
        assert!(
            !rendered
                .contains("impl :: boltffi :: __private :: ArcFromCallbackHandle for dyn Listener")
        );
    }

    #[test]
    fn native_callback_method_closure_return_expansion_reads_native_registration() {
        let source = closure_returning_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "pub make_handler : extern \"C\" fn (handle : u64 , __boltffi_return_out : * mut :: core :: ffi :: c_void) -> :: boltffi :: __private :: FfiStatus"
        ));
        assert!(rendered.contains("fn make_handler (& self) -> impl Fn (u32) -> u32"));
        assert!(rendered.contains(
            "let mut __boltffi_return_out = :: core :: mem :: MaybeUninit :: < __BoltffiCallbackClosureReturn__boltffi_closure > :: uninit ()"
        ));
        assert!(rendered.contains(
            "NativeCallbackOwner :: new (__boltffi_closure_context , __boltffi_closure_release)"
        ));
    }

    #[test]
    fn wasm_callback_method_closure_return_expansion_reads_wasm_handle() {
        let source = closure_returning_listener_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_wasm_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "fn __boltffi_callback_method_demo_listener_make_handler (handle : u32 , __boltffi_return_out : * mut u32) -> :: boltffi :: __private :: FfiStatus"
        ));
        assert!(rendered.contains("fn make_handler (& self) -> impl Fn (u32) -> u32"));
        assert!(rendered.contains(
            "let mut __boltffi_return_out = :: core :: mem :: MaybeUninit :: < u32 > :: uninit ()"
        ));
        assert!(rendered.contains("WasmCallbackOwner :: new (__boltffi_closure"));
    }

    #[test]
    fn native_callback_local_method_closure_return_expansion_writes_local_registration() {
        let source = boxed_closure_returning_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "extern \"C\" fn __boltffi_local_demo_listener_make_handler (handle : u64 , __boltffi_return_out : * mut :: core :: ffi :: c_void) -> :: boltffi :: __private :: FfiStatus"
        ));
        assert!(rendered.contains(
            "* __boltffi_return_out = __BoltffiClosureReturn__boltffi_closure { invoke : Some"
        ));
        assert!(rendered.contains("release : Some"));
    }

    #[test]
    fn native_callback_method_returning_callback_result_uses_local_success_handle() {
        let source = fallible_callback_returning_listener_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);

        let tokens =
            expand_native_callback(&expansion, &source.traits[0]).expect("expanded callback");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded callback parses");

        assert!(rendered.contains(
            "pub listener : extern \"C\" fn (handle : u64 , __boltffi_success_out : * mut :: boltffi :: __private :: CallbackHandle) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains(
            "let mut __boltffi_success_out = :: core :: mem :: MaybeUninit :: < :: boltffi :: __private :: CallbackHandle > :: uninit ()"
        ));
        assert!(rendered.contains("BoxFromCallbackHandle"));
        assert!(rendered.contains(
            "box_from_callback_handle (unsafe { __boltffi_success_out . assume_init () })"
        ));
        assert!(rendered.contains(
            "* __boltffi_success_out = __boltffi_local_demo_listener_handle (:: std :: sync :: Arc :: from (__boltffi_success))"
        ));
        assert!(!rendered.contains("boltffi_create_callback_demo_listener (__boltffi_success)"));
    }

    #[test]
    fn wasm_nullable_arc_callback_param_expansion_recovers_optional_shared_trait_object() {
        let source = nullable_arc_callback_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn maybe(listener: Option<std::sync::Arc<dyn Listener>>) -> u32 {
                listener.is_some() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn maybe(listener: Option<std::sync::Arc<dyn Listener> >) -> u32 {
                    listener.is_some() as u32
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_maybe(
                    listener: u32
                ) -> u32 {
                    let __boltffi_listener_handle =
                        ::boltffi::__private::CallbackHandle::from_wasm_handle(listener);
                    let listener: Option<::std::sync::Arc<dyn Listener> > =
                        if __boltffi_listener_handle.is_null() {
                            None
                        } else {
                            Some(unsafe {
                                <ForeignListener as ::boltffi::__private::ArcFromCallbackHandle>::arc_from_callback_handle(
                                    __boltffi_listener_handle
                                )
                            })
                        };
                    maybe(listener)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_boxed_callback_return_expansion_creates_callback_handle() {
        let source = boxed_callback_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_listener() -> Box<dyn Listener> {
                unimplemented!()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn make_listener() -> Box<dyn Listener> {
                    unimplemented!()
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_make_listener()
                    -> ::boltffi::__private::CallbackHandle
                {
                    let __boltffi_result: Box<dyn Listener> = make_listener();
                    __boltffi_local_demo_listener_handle(::std::sync::Arc::from(__boltffi_result))
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_arc_callback_return_expansion_reuses_shared_callback_handle() {
        let source = arc_callback_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn shared_listener() -> std::sync::Arc<dyn Listener> {
                unimplemented!()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn shared_listener() -> std::sync::Arc<dyn Listener> {
                    unimplemented!()
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_shared_listener()
                    -> ::boltffi::__private::CallbackHandle
                {
                    let __boltffi_result: ::std::sync::Arc<dyn Listener> = shared_listener();
                    __boltffi_local_demo_listener_handle(__boltffi_result)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_nullable_arc_callback_return_expansion_creates_optional_callback_handle() {
        let source = nullable_arc_callback_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn maybe_listener() -> Option<std::sync::Arc<dyn Listener>> {
                None
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn maybe_listener() -> Option<std::sync::Arc<dyn Listener> > {
                    None
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_maybe_listener() -> u32 {
                    let __boltffi_result: Option<::std::sync::Arc<dyn Listener> > =
                        maybe_listener();
                    __boltffi_result
                        .map(|__boltffi_callback|
                            __boltffi_local_demo_listener_handle(__boltffi_callback).handle() as u32
                        )
                        .unwrap_or(0)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_nullable_boxed_callback_return_expansion_creates_optional_callback_handle() {
        let source = nullable_boxed_callback_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn maybe_boxed_listener() -> Option<Box<dyn Listener>> {
                None
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn maybe_boxed_listener() -> Option<Box<dyn Listener> > {
                    None
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_maybe_boxed_listener() -> u32 {
                    let __boltffi_result: Option<Box<dyn Listener> > = maybe_boxed_listener();
                    __boltffi_result
                        .map(|__boltffi_callback| {
                            __boltffi_local_demo_listener_handle(
                                ::std::sync::Arc::from(__boltffi_callback)
                            ).handle() as u32
                        })
                        .unwrap_or(0)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_result_boxed_callback_return_expansion_writes_callback_success_out_pointer() {
        let source = result_boxed_callback_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_make_listener() -> Result<Box<dyn Listener>, String> {
                unimplemented!()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_make_listener() -> Result<Box<dyn Listener>, String> {
                    unimplemented!()
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_try_make_listener(
                    __boltffi_return_out: *mut ::boltffi::__private::CallbackHandle
                ) -> ::boltffi::__private::FfiBuf {
                    match try_make_listener() {
                        Ok(__boltffi_success) => {
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    ::core::ptr::write(
                                        __boltffi_return_out,
                                        __boltffi_local_demo_listener_handle(
                                            ::std::sync::Arc::from(__boltffi_success)
                                        )
                                    );
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error)
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_closure_param_expansion_builds_invoke_context_closure() {
        let source = closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn(u32) -> u32) -> u32 {
                callback(41)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn render(callback: impl Fn(u32) -> u32) -> u32 {
                    callback(41)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_apply(
                    __boltffi_callback_call: unsafe extern "C" fn(*mut ::core::ffi::c_void, u32) -> u32,
                    __boltffi_callback_context: *mut ::core::ffi::c_void,
                    __boltffi_callback_release: unsafe extern "C" fn(*mut ::core::ffi::c_void)
                ) -> u32 {
                    let __boltffi_callback_owner =
                        ::boltffi::__private::NativeCallbackOwner::new(
                            __boltffi_callback_context,
                            __boltffi_callback_release
                        );
                    let callback = move |__boltffi_arg0: u32| {
                        unsafe {
                            __boltffi_callback_call(
                                __boltffi_callback_owner.context(),
                                __boltffi_arg0
                            )
                        }
                    };
                    apply(callback)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_closure_param_expansion_builds_import_backed_closure() {
        let source = closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn(u32) -> u32) -> u32 {
                callback(41)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn render(callback: impl Fn(u32) -> u32) -> u32 {
                    callback(41)
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_apply(callback: u32) -> u32 {
                    unsafe extern "C" {
                        fn __boltffi_callback_closure____closure__u32_to_u32_call(
                            handle: u32,
                            __boltffi_ffi_arg0: u32
                        ) -> u32;
                        fn __boltffi_callback_closure____closure__u32_to_u32_free(handle: u32);
                    }
                    if callback == 0 {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null closure handle",
                            stringify!(callback)
                        ));
                        return <u32 as ::core::default::Default>::default();
                    }
                    let __boltffi_callback_owner =
                        ::boltffi::__private::WasmCallbackOwner::new(
                            callback,
                            __boltffi_callback_closure____closure__u32_to_u32_free
                        );
                    let callback = move |__boltffi_arg0: u32| {
                        unsafe {
                            __boltffi_callback_closure____closure__u32_to_u32_call(
                                __boltffi_callback_owner.handle(),
                                __boltffi_arg0
                            )
                        }
                    };
                    apply(callback)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_closure_param_expansion_decodes_encoded_invoke_return() {
        let source = string_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn(String) -> String) -> u32 {
                callback("x".to_string()).len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure param parses");

        assert!(rendered.contains("extern \"C\" fn (* mut :: core :: ffi :: c_void , * const u8 , usize) -> :: boltffi :: __private :: FfiBuf"));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_arg0)")
        );
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_result_bytes)"
        ));
        assert!(rendered.contains("apply (callback)"));
    }

    #[test]
    fn native_closure_param_expansion_decodes_custom_invoke_return_through_repr() {
        let source = custom_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn() -> Timestamp) -> u32 {
                callback().year()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded custom closure param parses");

        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < i64 > (__boltffi_result_bytes)"
        ));
        assert!(rendered.contains("(timestamp_try_from_ffi) (__boltffi_value)"));
        assert!(rendered.contains("(__boltffi_decoded)"));
        assert!(!rendered.contains("wire :: decode :: < Timestamp >"));
    }

    #[test]
    fn native_closure_param_expansion_decodes_fallible_custom_sequence_return_through_repr() {
        let source = fallible_custom_sequence_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Result<Vec<Timestamp>, String>) -> u32 {
                callback()
                    .map(|values| values.into_iter().map(|value| value.year()).sum())
                    .unwrap_or_default()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let generated = quote! {
            pub struct Timestamp(i64);

            impl Timestamp {
                fn year(&self) -> u32 {
                    self.0 as u32
                }
            }

            pub fn timestamp_into_ffi(value: &Timestamp) -> i64 {
                value.0
            }

            pub fn timestamp_try_from_ffi(value: i64) -> Result<Timestamp, ()> {
                Ok(Timestamp(value))
            }

            #tokens
        };

        assert_generated_crate_checks("native_custom_sequence_closure_param", generated);
        let rendered = tokens.to_string();
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < Vec < i64 > > (__boltffi_success_bytes)"
        ));
        assert!(rendered.contains("(timestamp_try_from_ffi) (value)"));
        assert!(!rendered.contains("wire :: decode :: < Vec < Timestamp > >"));
    }

    #[test]
    fn wasm_closure_param_expansion_decodes_fallible_custom_sequence_return_through_repr() {
        let source = fallible_custom_sequence_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Result<Vec<Timestamp>, String>) -> u32 {
                callback()
                    .map(|values| values.into_iter().map(|value| value.year()).sum())
                    .unwrap_or_default()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded function parses");

        assert!(rendered.contains(
            ":: boltffi :: __private :: take_packed_bytes (__boltffi_success . assume_init ())"
        ));
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < Vec < i64 > > (__boltffi_packed_bytes . as_slice ())"
        ));
        assert!(rendered.contains("(timestamp_try_from_ffi) (value)"));
        assert!(!rendered.contains("wire :: decode :: < Vec < Timestamp > >"));
    }

    #[test]
    fn wasm_closure_param_expansion_decodes_packed_string_invoke_return() {
        let source = string_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn(String) -> String) -> u32 {
                callback("x".to_string()).len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded wasm closure param parses");

        assert!(rendered.contains(
            "__boltffi_callback_closure____closure__string_to_string_call (__boltffi_callback_owner . handle ()"
        ));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: take_packed_bytes (__boltffi_result_packed)")
        );
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_packed_bytes . as_slice ())"
        ));
        assert!(!rendered.contains("take_packed_utf8_string"));
    }

    #[test]
    fn native_closure_param_expansion_decodes_optional_scalar_return() {
        let source = optional_scalar_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Option<i32>) -> i32 {
                callback().unwrap_or_default()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure param parses");

        assert!(rendered.contains(
            "extern \"C\" fn (* mut :: core :: ffi :: c_void) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(
            rendered.contains(":: boltffi :: __private :: wire :: decode :: < Option < i32 > >")
        );
    }

    #[test]
    fn wasm_closure_param_expansion_decodes_optional_scalar_return() {
        let source = optional_scalar_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Option<i32>) -> i32 {
                callback().unwrap_or_default()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure param parses");

        assert!(rendered.contains(") -> f64"));
        assert!(rendered.contains("if __boltffi_result . is_nan ()"));
    }

    #[test]
    fn native_closure_param_expansion_decodes_direct_vector_return() {
        let source = direct_vector_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Vec<u32>) -> u32 {
                callback().into_iter().sum()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure param parses");

        assert!(rendered.contains(
            "extern \"C\" fn (* mut :: core :: ffi :: c_void) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(
            rendered.contains("< u32 as :: boltffi :: __private :: VecTransport > :: unpack_vec")
        );
    }

    #[test]
    fn wasm_closure_param_expansion_decodes_direct_vector_return() {
        let source = direct_vector_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Vec<u32>) -> u32 {
                callback().into_iter().sum()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure param parses");

        assert!(rendered.contains("take_return_slot_vec :: < u32 >"));
    }

    #[test]
    fn native_closure_param_expansion_returns_fallible_void_result() {
        let source = fallible_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn() -> Result<(), String>) {
                let _ = callback();
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure param parses");

        assert!(
            rendered.contains("__boltffi_callback_call (__boltffi_callback_owner . context ())")
        );
        assert!(rendered.contains("if __boltffi_error_buf . is_empty () { Ok (()) }"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_error_bytes)"
        ));
    }

    #[test]
    fn wasm_closure_param_expansion_returns_fallible_void_result() {
        let source = fallible_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn() -> Result<(), String>) {
                let _ = callback();
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure param parses");

        assert!(rendered.contains(
            "__boltffi_callback_closure____closure__to_result_void_err_string_call (__boltffi_callback_owner . handle ())"
        ));
        assert!(rendered.contains("if __boltffi_error_packed == 0 { Ok (()) }"));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: take_packed_bytes (__boltffi_error_packed)")
        );
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_packed_bytes . as_slice ())"
        ));
    }

    #[test]
    fn native_closure_param_expansion_writes_fallible_direct_success_out_pointer() {
        let source = fallible_i32_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn() -> Result<i32, String>) {
                let _ = callback();
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure param parses");

        assert!(rendered.contains("extern \"C\" fn (* mut :: core :: ffi :: c_void , * mut i32) -> :: boltffi :: __private :: FfiBuf"));
        assert!(rendered.contains("MaybeUninit :: < i32 > :: uninit ()"));
        assert!(rendered.contains("Ok (unsafe { __boltffi_success . assume_init () })"));
        assert!(rendered.contains(":: boltffi :: __private :: wire :: decode :: < String >"));
    }

    #[test]
    fn wasm_closure_param_expansion_writes_fallible_encoded_success_out_pointer() {
        let source = fallible_string_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn render(callback: impl Fn() -> Result<String, String>) {
                let _ = callback();
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure param parses");

        assert!(rendered.contains("fn __boltffi_callback_closure____closure__to_result_string_err_string_call (handle : u32 , __boltffi_ffi_arg0 : * mut u64) -> u64"));
        assert!(rendered.contains("MaybeUninit :: < u64 > :: uninit ()"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: take_packed_bytes (__boltffi_success . assume_init ())"
        ));
        assert!(
            rendered
                .contains(":: boltffi :: __private :: take_packed_bytes (__boltffi_error_packed)")
        );
        assert!(rendered.contains(
            ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_packed_bytes . as_slice ())"
        ));
    }

    #[test]
    fn native_closure_return_expansion_writes_owned_context_out_pointer() {
        let source = closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> impl Fn(u32) -> u32 {
                |value| value + 1
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_function_demo_make_callback (__boltffi_return_out : * mut :: core :: ffi :: c_void) -> :: boltffi :: __private :: FfiStatus"
        ));
        assert!(rendered.contains(
            "struct __BoltffiClosureReturn__boltffi_closure { invoke : Option < unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void , u32) -> u32 > , context : * mut :: core :: ffi :: c_void , release : Option < unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void) > , }"
        ));
        assert!(rendered.contains(
            "* __boltffi_return_out = __BoltffiClosureReturn__boltffi_closure { invoke : Some (__boltffi_make_callback_closure_call) , context : __boltffi_closure_context , release : Some (__boltffi_make_callback_closure_release) , }"
        ));
    }

    #[test]
    fn wasm_closure_return_expansion_exports_call_free_and_writes_handle() {
        let source = closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> impl Fn(u32) -> u32 {
                |value| value + 1
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_function_demo_make_callback (__boltffi_return_out : * mut u32) -> :: boltffi :: __private :: FfiStatus"
        ));
        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__u32_to_u32_call (__boltffi_context : u32 , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__u32_to_u32_free (__boltffi_context : u32)"
        ));
        assert!(rendered.contains(
            "* __boltffi_return_out = Box :: into_raw (Box :: new (Box :: new (__boltffi_closure) as Box < dyn Fn (u32) -> u32 + 'static >)) as usize as u32"
        ));
    }

    #[test]
    fn native_async_boxed_closure_return_expansion_writes_owned_context_out_pointer() {
        let source = async_boxed_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn make_callback() -> Box<dyn Fn(u32) -> u32> {
                Box::new(|value| value + 1)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded async closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_async_function_demo_make_callback_complete"
        ));
        assert!(rendered.contains("__boltffi_return_out : * mut :: core :: ffi :: c_void"));
        assert!(rendered.contains("-> :: boltffi :: __private :: FfiStatus"));
        assert!(rendered.contains("closure return out pointer is null"));
        assert!(rendered.contains("__boltffi_make_callback_closure_call"));
        assert!(rendered.contains("* __boltffi_return_out = __BoltffiClosureReturn"));
    }

    #[test]
    fn wasm_async_boxed_closure_return_expansion_writes_wasm_handle_out_pointer() {
        let source = async_boxed_closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn make_callback() -> Box<dyn Fn(u32) -> u32> {
                Box::new(|value| value + 1)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded async closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_async_function_demo_make_callback_complete"
        ));
        assert!(rendered.contains("__boltffi_return_out : * mut u32"));
        assert!(rendered.contains("-> :: boltffi :: __private :: FfiStatus"));
        assert!(rendered.contains("closure return out pointer is null"));
        assert!(rendered.contains("boltffi_closure_"));
        assert!(rendered.contains("* __boltffi_return_out = Box :: into_raw"));
    }

    #[test]
    fn wasm_closure_return_invoke_reads_direct_record_param_from_pointer() {
        let source = closure_return_with_record_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> impl Fn(Point) -> u32 {
                |point| point.x as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_closure_1____closure__demo_point_to_u32_call (__boltffi_context : u32 , __boltffi_arg0 : * const u8) -> u32"
        ));
        assert!(rendered.contains(
            "let __boltffi_arg0 : Point = unsafe { let __boltffi_value = :: core :: ptr :: read_unaligned (__boltffi_arg0 as * const < Point as :: boltffi :: __private :: Passable > :: In) ; < Point as :: boltffi :: __private :: Passable > :: unpack (__boltffi_value) }"
        ));
        assert!(
            !rendered.contains(
                "__boltffi_arg0 : < Point as :: boltffi :: __private :: Passable > :: In"
            )
        );
    }

    #[test]
    fn native_function_pointer_closure_return_expansion_boxes_pointer_context() {
        let source = function_pointer_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> fn(u32) -> u32 {
                fn increment(value: u32) -> u32 {
                    value + 1
                }
                increment
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded function-pointer closure parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "unsafe extern \"C\" fn __boltffi_make_callback_closure_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(
            rendered.contains(
                "Box :: new (__boltffi_closure) as Box < dyn Fn (u32) -> u32 + 'static >"
            )
        );
        assert!(rendered.contains(
            "struct __BoltffiClosureReturn__boltffi_closure { invoke : Option < unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void , u32) -> u32 > , context : * mut :: core :: ffi :: c_void , release : Option < unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void) > , }"
        ));
    }

    #[test]
    fn wasm_function_pointer_closure_return_expansion_boxes_pointer_context() {
        let source = function_pointer_closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> fn(u32) -> u32 {
                fn increment(value: u32) -> u32 {
                    value + 1
                }
                increment
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded function-pointer closure parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__u32_to_u32_call (__boltffi_context : u32 , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(rendered.contains(
            "* __boltffi_return_out = Box :: into_raw (Box :: new (Box :: new (__boltffi_closure) as Box < dyn Fn (u32) -> u32 + 'static >)) as usize as u32"
        ));
    }

    #[test]
    fn native_closure_return_invoke_accepts_closure_parameter() {
        let source = closure_return_with_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_runner() -> impl Fn(Box<dyn Fn(u32) -> u32>) -> u32 {
                |callback| callback(41)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded nested closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "unsafe extern \"C\" fn __boltffi_make_runner_closure_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_arg0_call : unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void , u32) -> u32 , __boltffi_arg0_context : * mut :: core :: ffi :: c_void , __boltffi_arg0_release : unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void)) -> u32"
        ));
        assert!(rendered.contains(
            "let __boltffi_arg0 : Box < dyn Fn (u32) -> u32 > = Box :: new (move | __boltffi_arg0 : u32 |"
        ));
        assert!(
            rendered.contains(
                "__boltffi_arg0_call (__boltffi_arg0_owner . context () , __boltffi_arg0)"
            )
        );
    }

    #[test]
    fn wasm_closure_return_invoke_accepts_closure_parameter() {
        let source = closure_return_with_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_runner() -> impl Fn(Box<dyn Fn(u32) -> u32>) -> u32 {
                |callback| callback(41)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded nested closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_closure_1____closure__box_closure_to_u32_call (__boltffi_context : u32 , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(rendered.contains(
            "unsafe extern \"C\" { fn __boltffi_callback_closure____closure__u32_to_u32_call (handle : u32 , __boltffi_ffi_arg0 : u32) -> u32 ; fn __boltffi_callback_closure____closure__u32_to_u32_free (handle : u32) ; }"
        ));
        assert!(rendered.contains(
            "__boltffi_callback_closure____closure__u32_to_u32_call (__boltffi_arg0_owner . handle () , __boltffi_arg0)"
        ));
    }

    #[test]
    fn native_encoded_closure_return_invoke_decodes_arg_and_returns_buffer() {
        let source = string_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_mapper() -> impl Fn(String) -> String {
                |value| value
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded encoded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "unsafe extern \"C\" fn __boltffi_make_mapper_closure_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_arg0_ptr : * const u8 , __boltffi_arg0_len : usize) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains("let __boltffi_arg0 : String = {"));
        assert!(
            rendered.contains(
                ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_bytes)"
            )
        );
        assert!(
            rendered
                .contains(":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_result)")
        );
    }

    #[test]
    fn native_closure_return_invoke_encodes_custom_return_through_repr() {
        let source = custom_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_clock() -> impl Fn() -> Timestamp {
                || Timestamp::now()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded custom closure return parses");

        assert!(
            rendered.contains("let __boltffi_wire = (timestamp_into_ffi) (& __boltffi_result)")
        );
        assert!(
            rendered
                .contains(":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_wire)")
        );
        assert!(!rendered.contains("FfiBuf :: wire_encode (& __boltffi_result)"));
    }

    #[test]
    fn wasm_encoded_closure_return_invoke_decodes_arg_and_returns_packed_buffer() {
        let source = string_closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_mapper() -> impl Fn(String) -> String {
                |value| value
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded encoded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__string_to_string_call (__boltffi_context : u32 , __boltffi_arg0_ptr : * const u8 , __boltffi_arg0_len : usize) -> u64"
        ));
        assert!(rendered.contains("let __boltffi_arg0 : String = {"));
        assert!(
            rendered.contains(
                ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_bytes)"
            )
        );
        assert!(rendered.contains(
            ":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_result) . into_packed ()"
        ));
    }

    #[test]
    fn native_result_closure_return_expansion_writes_closure_success_and_encoded_error() {
        let source = result_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_make_callback() -> Result<Box<dyn Fn(u32) -> u32>, String> {
                Ok(Box::new(|value| value + 1))
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure result parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_function_demo_try_make_callback (__boltffi_return_out : * mut :: core :: ffi :: c_void) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains(
            "Ok (__boltffi_success) => { # [repr (C)] struct __BoltffiClosureReturn__boltffi_success"
        ));
        assert!(
            rendered.contains("invoke : Some (__boltffi_try_make_callback_success_closure_call)")
        );
        assert!(
            rendered
                .contains("release : Some (__boltffi_try_make_callback_success_closure_release)")
        );
        assert!(rendered.contains(
            "Err (__boltffi_error) => { :: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_error) }"
        ));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: default ()"));
    }

    #[test]
    fn native_closure_return_invoke_writes_fallible_direct_success_out_pointer() {
        let source = fallible_i32_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> impl Fn() -> Result<i32, String> {
                || Ok(7)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure invoke parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "unsafe extern \"C\" fn __boltffi_make_callback_closure_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_success_out : * mut i32) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains("match __boltffi_closure ()"));
        assert!(rendered.contains("* __boltffi_success_out = __boltffi_success"));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: default ()"));
        assert!(rendered.contains(
            "Err (__boltffi_error) => { :: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_error) }"
        ));
    }

    #[test]
    fn wasm_closure_return_invoke_writes_fallible_encoded_success_out_pointer() {
        let source = fallible_string_closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_mapper() -> impl Fn() -> Result<String, String> {
                || Ok("x".to_string())
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure invoke parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__to_result_string_err_string_call (__boltffi_context : u32 , __boltffi_success_out : * mut u64) -> u64"
        ));
        assert!(rendered.contains("match __boltffi_closure ()"));
        assert!(rendered.contains(
            "* __boltffi_success_out = :: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_success) . into_packed ()"
        ));
        assert!(rendered.contains(
            "Err (__boltffi_error) => { :: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_error) . into_packed () }"
        ));
    }

    #[test]
    fn wasm_borrowed_class_param_expansion_borrows_required_handle() {
        let source = borrowed_class_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn engine_id(engine: &Engine) -> u32 {
                7
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn engine_id(engine: &Engine) -> u32 {
                    7
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_engine_id(
                    engine: u32
                ) -> u32 {
                    if engine == 0 {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null class handle",
                            stringify!(engine)
                        ));
                        return <u32 as ::core::default::Default>::default();
                    }
                    let engine: &Engine = unsafe {
                        __BoltffiEngineHandle::shared(engine as usize as *mut __BoltffiEngineHandle)
                    };
                    engine_id(engine)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_result_class_string_expansion_writes_success_handle_out_pointer() {
        let source = result_class_string_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_open() -> Result<Engine, String> {
                Ok(Engine)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_open() -> Result<Engine, String> {
                    Ok(Engine)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_try_open(
                    __boltffi_return_out: *mut u64
                ) -> ::boltffi::__private::FfiBuf {
                    match try_open() {
                        Ok(__boltffi_success) => {
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    ::core::ptr::write(
                                        __boltffi_return_out,
                                        __BoltffiEngineHandle::new(__boltffi_success) as usize as u64
                                    );
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error)
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_option_i32_param_expansion_decodes_nan_sentinel_scalar() {
        let source = option_i32_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn set_count(count: Option<i32>) {}
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn set_count(count: Option<i32>) {}
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_set_count(
                    count: f64
                ) -> ::boltffi::__private::FfiStatus {
                    let count: Option<i32> = if count.is_nan() {
                        None
                    } else {
                        Some(count as _)
                    };
                    set_count(count);
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_vec_u32_param_expansion_decodes_direct_vector() {
        let source = vec_u32_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn sum(values: Vec<u32>) -> u32 {
                values.into_iter().sum()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn sum(values: Vec<u32>) -> u32 {
                    values.into_iter().sum()
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_sum(
                    __boltffi_values_ptr: *const u32,
                    __boltffi_values_len: usize
                ) -> u32 {
                    let values: Vec<u32> = if __boltffi_values_ptr.is_null() {
                        Vec::new()
                    } else {
                        unsafe {
                            ::core::slice::from_raw_parts(
                                __boltffi_values_ptr,
                                __boltffi_values_len
                            )
                        }.to_vec()
                    };
                    sum(values)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_vec_record_param_expansion_decodes_passable_vector() {
        let source = vec_point_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn count_points(points: Vec<Point>) -> u32 {
                points.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn count_points(points: Vec<Point>) -> u32 {
                    points.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_count_points(
                    __boltffi_points_ptr: *const u8,
                    __boltffi_points_len: usize
                ) -> u32 {
                    let points: Vec<Point> = if __boltffi_points_ptr.is_null() {
                        Vec::new()
                    } else {
                        let raw_byte_len = __boltffi_points_len;
                        let element_size =
                            ::core::mem::size_of::<<Point as ::boltffi::__private::Passable>::In>();
                        if raw_byte_len % element_size == 0 {
                            unsafe {
                                <Point as ::boltffi::__private::VecTransport>::unpack_vec(
                                    __boltffi_points_ptr,
                                    raw_byte_len
                                )
                            }
                        } else {
                            ::boltffi::__private::set_last_error(format!(
                                "invalid byte length {} for Vec<{}>: not divisible by element size {}",
                                raw_byte_len,
                                ::core::any::type_name::<Point>(),
                                element_size
                            ));
                            return <u32 as ::core::default::Default>::default();
                        }
                    };
                    count_points(points)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn direct_record_return_expansion_packs_passable_output() {
        let source = direct_record_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn origin() -> Point {
                Point { x: 0.0 }
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn origin() -> Point {
                    Point { x: 0.0 }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_origin() -> <Point as ::boltffi::__private::Passable>::Out {
                    <Point as ::boltffi::__private::Passable>::pack(origin())
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_result_i32_string_expansion_writes_success_out_pointer() {
        let source = result_i32_string_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_count() -> Result<i32, String> {
                Ok(7)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_count() -> Result<i32, String> {
                    Ok(7)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_try_count(
                    __boltffi_return_out: *mut i32
                ) -> ::boltffi::__private::FfiBuf {
                    match try_count() {
                        Ok(__boltffi_success) => {
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    ::core::ptr::write(
                                        __boltffi_return_out,
                                        __boltffi_success
                                    );
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error)
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_result_unit_string_expansion_returns_encoded_error_status() {
        let source = result_unit_string_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_ping() -> Result<(), String> {
                Ok(())
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_ping() -> Result<(), String> {
                    Ok(())
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_try_ping() -> u64 {
                    match try_ping() {
                        Ok(()) => {
                            ::boltffi::__private::FfiBuf::default().into_packed()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error).into_packed()
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_result_string_string_expansion_writes_encoded_success_out_pointer() {
        let source = result_string_string_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_greet() -> Result<String, String> {
                Ok(String::from("hello"))
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_greet() -> Result<String, String> {
                    Ok(String::from("hello"))
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_try_greet(
                    __boltffi_return_out: *mut u64
                ) -> u64 {
                    match try_greet() {
                        Ok(__boltffi_success) => {
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    ::core::ptr::write(
                                        __boltffi_return_out,
                                        ::boltffi::__private::FfiBuf::wire_encode(
                                            &__boltffi_success
                                        ).into_packed()
                                    );
                                }
                            }
                            ::boltffi::__private::FfiBuf::default().into_packed()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error).into_packed()
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_option_i32_return_expansion_returns_encoded_option_buffer() {
        let source = option_i32_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn maybe_count() -> Option<i32> {
                Some(7)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn maybe_count() -> Option<i32> {
                    Some(7)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_maybe_count() -> ::boltffi::__private::FfiBuf {
                    let __boltffi_result: Option<i32> = maybe_count();
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_option_i32_return_expansion_returns_nan_sentinel_scalar() {
        let source = option_i32_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn maybe_count() -> Option<i32> {
                Some(7)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn maybe_count() -> Option<i32> {
                    Some(7)
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_maybe_count() -> f64 {
                    let __boltffi_result: Option<i32> = maybe_count();
                    match __boltffi_result {
                        Some(__boltffi_value) => __boltffi_value as f64,
                        None => f64::NAN,
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_vec_i32_return_expansion_returns_direct_vector_buffer() {
        let source = vec_i32_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn numbers() -> Vec<i32> {
                vec![1, 2, 3]
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn numbers() -> Vec<i32> {
                    vec![1, 2, 3]
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_numbers() -> ::boltffi::__private::FfiBuf {
                    let __boltffi_result = numbers();
                    <i32 as ::boltffi::__private::VecTransport>::pack_vec(__boltffi_result)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_vec_i32_return_expansion_writes_direct_vector_return_slot() {
        let source = vec_i32_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn numbers() -> Vec<i32> {
                vec![1, 2, 3]
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn numbers() -> Vec<i32> {
                    vec![1, 2, 3]
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_numbers() {
                    let __boltffi_result = numbers();
                    let __boltffi_buf =
                        <i32 as ::boltffi::__private::VecTransport>::pack_vec(__boltffi_result);
                    ::boltffi::__private::write_return_slot(
                        __boltffi_buf.as_ptr() as u32,
                        __boltffi_buf.len() as u32,
                        __boltffi_buf.cap() as u32,
                        __boltffi_buf.align() as u32
                    );
                    core::mem::forget(__boltffi_buf);
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_string_return_expansion_returns_buffer() {
        let source = string_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn greet() -> String {
                String::from("hello")
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn greet() -> String {
                    String::from("hello")
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_greet() -> ::boltffi::__private::FfiBuf {
                    let __boltffi_result: String = greet();
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_string_return_expansion_returns_packed_buffer() {
        let source = string_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn greet() -> String {
                String::from("hello")
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn greet() -> String {
                    String::from("hello")
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_greet() -> u64 {
                    let __boltffi_result: String = greet();
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result).into_packed()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_bytes_return_expansion_uses_wire_framing() {
        let source = bytes_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn payload() -> Vec<u8> {
                vec![1, 2, 3]
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn payload() -> Vec<u8> {
                    vec![1, 2, 3]
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_payload() -> u64 {
                    let __boltffi_result: Vec<u8> = payload();
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result).into_packed()
                }
            }
            .to_string()
        );
    }
}
