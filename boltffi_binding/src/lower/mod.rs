//! Lowers a scanned Rust source contract into a binding contract for a
//! target [`Surface`].
//!
//! The pass runs once. The returned [`Bindings<S>`] contains the
//! decisions consumers render: direct records carry layout, encoded
//! records carry codec plans, and enums have already been split into
//! c-style or data-bearing forms. Source shapes that do not have a
//! binding-IR representation yet return [`LowerError`] instead of being
//! guessed.
//!
//! # Pipeline
//!
//! 1. Build [`DeclarationIds`] from the source. Duplicate ids in the
//!    same family fail here, before any walk.
//! 2. Build an [`Index`] of the source for cross-decl lookups during
//!    type and codec lowering.
//! 3. Lower every record into [`RecordDecl<S>`], every enum into
//!    [`EnumDecl<S>`], every class into [`ClassDecl<S>`], every trait
//!    into [`CallbackDecl<S>`] (the trait's per-surface dispatch
//!    protocol), every free function into [`FunctionDecl<S>`], every
//!    stream into [`StreamDecl<S>`], every custom type into
//!    [`CustomTypeDecl`], and every constant into [`ConstantDecl<S>`].
//! 4. Hand the collected decls to [`Bindings::from_decls`], which
//!    derives the native symbol table from the symbols the decls
//!    reference and validates the result.
//!
//! Each step in the pipeline returns either final IR or the
//! infrastructure the next step uses; nothing returns a private
//! domain-shaped middle value.
//!
//! The surface is selected at the call site:
//!
//! ```ignore
//! let native = boltffi_binding::lower::<boltffi_binding::Native>(&source)?;
//! let wasm   = boltffi_binding::lower::<boltffi_binding::Wasm32>(&source)?;
//! ```

#![allow(dead_code)]

mod async_protocol;
mod callable;
mod callbacks;
mod classes;
mod codecs;
mod constants;
mod customs;
mod enums;
mod error;
mod functions;
mod ids;
mod index;
mod layout;
mod metadata;
mod methods;
mod names;
mod opaque;
mod primitive;
mod records;
mod streams;
mod surface;
mod symbol;
mod types;
mod wasm_closure;

use boltffi_ast::SourceContract;

use crate::{BindingError, Bindings, CanonicalName, Decl, PackageInfo};

pub use self::error::{DeclarationFamily, LowerError, LowerErrorKind, UnsupportedType};
pub use self::surface::SurfaceLower;

use self::{ids::DeclarationIds, index::Index, symbol::SymbolAllocator};

pub use self::ids::DeclarationMap;

/// Binding contract plus the source declaration ids that produced it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoweredBindings<S: crate::Surface> {
    bindings: Bindings<S>,
    declarations: DeclarationMap,
}

impl<S: crate::Surface> LoweredBindings<S> {
    /// Builds a lowered binding result.
    pub fn new(bindings: Bindings<S>, declarations: DeclarationMap) -> Self {
        Self {
            bindings,
            declarations,
        }
    }

    /// Returns the binding contract.
    pub fn bindings(&self) -> &Bindings<S> {
        &self.bindings
    }

    /// Returns the source-to-binding declaration map.
    pub fn declarations(&self) -> &DeclarationMap {
        &self.declarations
    }

    /// Returns the binding contract.
    pub fn into_bindings(self) -> Bindings<S> {
        self.bindings
    }
}

/// Lowers a source contract into a binding contract for surface `S`.
///
/// See the module-level docs for the steps each call runs through.
pub fn lower<S: SurfaceLower>(source: &SourceContract) -> Result<Bindings<S>, LowerError> {
    lower_with_declarations(source).map(LoweredBindings::into_bindings)
}

/// Lowers a source contract and keeps the source-to-binding declaration map.
pub fn lower_with_declarations<S: SurfaceLower>(
    source: &SourceContract,
) -> Result<LoweredBindings<S>, LowerError> {
    let ids = DeclarationIds::from_source(source)?;
    let bindings = lower_with_ids::<S>(source, &ids)?;
    let declarations = ids.declaration_map();
    Ok(LoweredBindings::new(bindings, declarations))
}

fn lower_with_ids<S: SurfaceLower>(
    source: &SourceContract,
    ids: &DeclarationIds,
) -> Result<Bindings<S>, LowerError> {
    let index = Index::new(source);
    let mut allocator = SymbolAllocator::new();

    let records = records::lower::<S>(&index, ids, &mut allocator)?;
    let enums = enums::lower::<S>(&index, ids, &mut allocator)?;
    let classes = classes::lower::<S>(&index, ids, &mut allocator)?;
    let callbacks = callbacks::lower::<S>(&index, ids, &mut allocator)?;
    let functions = functions::lower::<S>(&index, ids, &mut allocator)?;
    let streams = streams::lower::<S>(&index, ids, &mut allocator)?;
    let constants = constants::lower::<S>(&index, ids, &mut allocator)?;
    let customs = customs::lower(&index, ids)?;

    let decls = records
        .into_iter()
        .map(|record| Decl::Record(Box::new(record)))
        .chain(
            enums
                .into_iter()
                .map(|enumeration| Decl::Enum(Box::new(enumeration))),
        )
        .chain(
            classes
                .into_iter()
                .map(|class| Decl::Class(Box::new(class))),
        )
        .chain(
            callbacks
                .into_iter()
                .map(|callback| Decl::Callback(Box::new(callback))),
        )
        .chain(
            functions
                .into_iter()
                .map(|function| Decl::Function(Box::new(function))),
        )
        .chain(
            streams
                .into_iter()
                .map(|stream| Decl::Stream(Box::new(stream))),
        )
        .chain(
            customs
                .into_iter()
                .map(|custom| Decl::CustomType(Box::new(custom))),
        )
        .chain(
            constants
                .into_iter()
                .map(|constant| Decl::Constant(Box::new(constant))),
        )
        .collect::<Vec<_>>();

    let package = PackageInfo::new(
        CanonicalName::single(source.package.name.as_str()),
        source.package.version.clone(),
    );

    Ok(Bindings::from_decls(package, decls)?)
}

impl From<BindingError> for LowerError {
    fn from(error: BindingError) -> Self {
        Self::new(LowerErrorKind::InvalidBindings(error))
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        BuiltinType, CanonicalName as SourceName, DeclarationId as SourceDeclarationId,
        FunctionDef as SourceFunction, FunctionId as SourceFunctionId,
        PackageInfo as SourcePackage, ParameterDef as SourceParameter,
        Primitive as SourcePrimitive, ReturnDef as SourceReturn, SourceContract,
        TypeExpr as SourceType,
    };

    use crate::{
        CanonicalName, CodecNode, Decl, DeclarationId, FunctionId, Native, ParamPlan, ReadPlan,
        ReturnPlan, TypeRef, ValueRef, WritePlan, native,
    };

    use super::{lower, lower_with_declarations};

    fn source_contract() -> SourceContract {
        let mut function = SourceFunction::new(
            SourceFunctionId::new("demo::answer"),
            SourceName::single("answer"),
        );
        function.returns = SourceReturn::value(SourceType::Primitive(SourcePrimitive::U32));

        let mut source = SourceContract::new(SourcePackage::new("demo", None));
        source.functions.push(function);
        source
    }

    #[test]
    fn lower_with_declarations_preserves_function_identity() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");

        assert_eq!(
            lowered
                .declarations()
                .get(&SourceDeclarationId::Function(SourceFunctionId::new(
                    "demo::answer"
                ))),
            Some(DeclarationId::Function(FunctionId::from_raw(0)))
        );
    }

    #[test]
    fn lowers_builtin_values_as_encoded_wire_values() {
        let mut function = SourceFunction::new(
            SourceFunctionId::new("demo::echo_duration"),
            SourceName::single("echo_duration"),
        );
        function.parameters = vec![SourceParameter::value(
            SourceName::single("value"),
            SourceType::builtin(BuiltinType::Duration),
        )];
        function.returns = SourceReturn::value(SourceType::builtin(BuiltinType::Duration));

        let mut source = SourceContract::new(SourcePackage::new("demo", None));
        source.functions.push(function);

        let bindings = lower::<Native>(&source).expect("lowered bindings");
        let function = match bindings.decls().first() {
            Some(Decl::Function(function)) => function,
            other => panic!("expected function declaration, got {other:?}"),
        };

        assert_eq!(
            function.callable().params()[0].as_value().unwrap(),
            &ParamPlan::Encoded {
                ty: TypeRef::Builtin(BuiltinType::Duration),
                codec: WritePlan::new(
                    ValueRef::named(CanonicalName::single("value")),
                    CodecNode::Builtin(BuiltinType::Duration)
                ),
                shape: native::BufferShape::Slice,
                receive: crate::Receive::ByValue,
            }
        );
        assert_eq!(
            function.callable().returns().plan(),
            &ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Builtin(BuiltinType::Duration),
                codec: ReadPlan::new(CodecNode::Builtin(BuiltinType::Duration)),
                shape: native::BufferShape::Buffer,
            }
        );
    }
}
