//! Method and initializer lowering for declaration-owned callables.
//!
//! Records, enums, and classes can promote static `Self`-returning methods to
//! [`InitializerDecl<S>`]. Records, enums, and classes keep every other
//! method as a [`ExportedMethodDecl<S, NativeSymbol>`]. The callable body is
//! lowered by [`super::callable`]; this module owns the initializer
//! discriminator, target symbol allocation, and owner-specific constructed
//! type recorded on an initializer.

use boltffi_ast::{
    ClassDef, EnumDef, MethodDef, Receiver, RecordDef, ReturnDef, TraitDef, TypeExpr,
};

use crate::{
    CanonicalName, ExecutionDecl, ExportedMethodDecl, ImportedMethodDecl, InitializerDecl,
    InitializerId, MethodId, NativeSymbol, ReturnTypeRef, TypeRef,
};

use super::{
    LowerError, callable,
    error::UnsupportedType,
    ids::DeclarationIds,
    index::Index,
    metadata,
    surface::SurfaceLower,
    symbol::{CallbackSlot, SymbolAllocator, SymbolOwner},
};

/// Lowers every initializer-shaped method on `record`.
///
/// Initializer ids are assigned after non-initializer methods are removed,
/// so the initializer table is dense in the exact order renderers observe.
pub fn lower_record_initializers<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    record: &RecordDef,
) -> Result<Vec<InitializerDecl<S>>, LowerError> {
    let owner = callable::CallableOwner::Record(record);
    let record_id = ids.record(&record.id)?;
    lower_initializers(
        index,
        ids,
        allocator,
        owner,
        &record.methods,
        TypeRef::Record(record_id),
    )
}

/// Lowers every non-initializer method on `record`.
///
/// Method ids are assigned after initializer-shaped methods are removed,
/// so the method table is dense in the exact order renderers observe.
pub fn lower_record_methods<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    record: &RecordDef,
) -> Result<Vec<ExportedMethodDecl<S, NativeSymbol>>, LowerError> {
    let owner = callable::CallableOwner::Record(record);
    record
        .methods
        .iter()
        .filter(|method| !is_initializer(owner, method))
        .enumerate()
        .map(|(method_index, method)| {
            lower_method::<S>(
                index,
                ids,
                allocator,
                owner,
                method,
                MethodId::from_raw(method_index as u32),
            )
        })
        .collect()
}

/// Lowers every initializer-shaped method on `enumeration`.
pub fn lower_enum_initializers<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    enumeration: &EnumDef,
) -> Result<Vec<InitializerDecl<S>>, LowerError> {
    let owner = callable::CallableOwner::Enum(enumeration);
    let enum_id = ids.enumeration(&enumeration.id)?;
    lower_initializers(
        index,
        ids,
        allocator,
        owner,
        &enumeration.methods,
        TypeRef::Enum(enum_id),
    )
}

/// Lowers every non-initializer method on `enumeration`.
pub fn lower_enum_methods<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    enumeration: &EnumDef,
) -> Result<Vec<ExportedMethodDecl<S, NativeSymbol>>, LowerError> {
    let owner = callable::CallableOwner::Enum(enumeration);
    enumeration
        .methods
        .iter()
        .filter(|method| !is_initializer(owner, method))
        .enumerate()
        .map(|(method_index, method)| {
            lower_method::<S>(
                index,
                ids,
                allocator,
                owner,
                method,
                MethodId::from_raw(method_index as u32),
            )
        })
        .collect()
}

/// Lowers every initializer-shaped method on `class`.
///
/// Class initializers construct the class handle target rather than a
/// value-shaped record. The callable still carries the native crossing
/// selected for the `Self` return.
pub fn lower_class_initializers<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    class: &ClassDef,
) -> Result<Vec<InitializerDecl<S>>, LowerError> {
    let owner = callable::CallableOwner::Class(class);
    let class_id = ids.class(&class.id)?;
    lower_initializers(
        index,
        ids,
        allocator,
        owner,
        &class.methods,
        TypeRef::Class(class_id),
    )
}

/// Lowers every non-initializer method on `class`.
///
/// Owned class receivers are rejected until the handle ownership-transfer
/// protocol is represented in the binding IR.
pub fn lower_class_methods<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    class: &ClassDef,
) -> Result<Vec<ExportedMethodDecl<S, NativeSymbol>>, LowerError> {
    let owner = callable::CallableOwner::Class(class);
    class
        .methods
        .iter()
        .filter(|method| !is_initializer(owner, method))
        .enumerate()
        .map(|(method_index, method)| {
            reject_owned_class_receiver(method)?;
            lower_method::<S>(
                index,
                ids,
                allocator,
                owner,
                method,
                MethodId::from_raw(method_index as u32),
            )
        })
        .collect()
}

fn is_initializer(owner: callable::CallableOwner, method: &MethodDef) -> bool {
    matches!(method.receiver, Receiver::None)
        && match &method.returns {
            ReturnDef::Value(type_expr) => returns_owner(owner, type_expr),
            ReturnDef::Void => false,
        }
}

fn returns_owner(owner: callable::CallableOwner, type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Result { ok, .. } => owner.owns_type_expr(ok),
        other => owner.owns_type_expr(other),
    }
}

fn lower_initializers<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner,
    methods: &[MethodDef],
    returns: TypeRef,
) -> Result<Vec<InitializerDecl<S>>, LowerError> {
    methods
        .iter()
        .filter(|method| is_initializer(owner, method))
        .enumerate()
        .map(|(initializer_index, method)| {
            lower_initializer(
                index,
                ids,
                allocator,
                owner,
                method,
                InitializerId::from_raw(initializer_index as u32),
                returns.clone(),
            )
        })
        .collect()
}

fn lower_initializer<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner,
    method: &MethodDef,
    id: InitializerId,
    returns: TypeRef,
) -> Result<InitializerDecl<S>, LowerError> {
    let symbol = allocator.mint_initializer(symbol_owner(owner), &method.name)?;
    let callable_decl = callable::lower_exported_method::<S>(
        index,
        ids,
        allocator,
        owner,
        method,
        symbol.name().as_str(),
    )?;
    Ok(InitializerDecl::new(
        id,
        CanonicalName::from(&method.name),
        metadata::decl_meta(method.doc.as_ref(), method.deprecated.as_ref()),
        symbol,
        callable_decl,
        ReturnTypeRef::Value(returns),
    ))
}

fn lower_method<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner,
    method: &MethodDef,
    id: MethodId,
) -> Result<ExportedMethodDecl<S, NativeSymbol>, LowerError> {
    let symbol = allocator.mint_method(symbol_owner(owner), &method.name)?;
    let callable_decl = callable::lower_exported_method::<S>(
        index,
        ids,
        allocator,
        owner,
        method,
        symbol.name().as_str(),
    )?;
    Ok(ExportedMethodDecl::new(
        id,
        CanonicalName::from(&method.name),
        metadata::decl_meta(method.doc.as_ref(), method.deprecated.as_ref()),
        symbol,
        callable_decl,
    ))
}

/// Lowers every method on `callback` with a per-surface dispatch
/// target.
///
/// Records and enums and classes all use [`NativeSymbol`] as the
/// method target because their methods are Rust-implemented. Callback
/// traits invert ownership: foreign code provides each method, so the
/// target is surface-divergent — a [`crate::VTableSlot`] on native, a
/// [`crate::ImportSymbol`] on wasm32.
///
/// The `target_for` closure receives a [`CallbackSlot`], not a raw
/// string. The slot is constructed once here from the source method
/// ident, so the type system guarantees the value handed to each
/// surface is the canonical snake-cased name. A future caller of the
/// per-surface name constructors cannot bypass normalization because
/// they cannot construct a [`CallbackSlot`] from arbitrary text.
pub fn lower_callback_methods<S: SurfaceLower, T, F>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    source_trait: &TraitDef,
    mut surface_for: F,
) -> Result<Vec<ImportedMethodDecl<S, T>>, LowerError>
where
    F: FnMut(
        &mut SymbolAllocator,
        &MethodDef,
        &CallbackSlot,
    ) -> Result<CallbackMethodSurface<S, T>, LowerError>,
{
    let owner = callable::CallableOwner::Trait(source_trait);
    source_trait
        .methods
        .iter()
        .enumerate()
        .map(|(method_index, method)| {
            require_callback_receiver(method.receiver)?;
            let slot = CallbackSlot::from_source_name(&method.name);
            let surface = surface_for(allocator, method, &slot)?;
            let callable_decl = callable::lower_imported_method::<S>(
                index,
                ids,
                allocator,
                owner,
                method,
                surface.execution,
            )?;
            Ok(ImportedMethodDecl::new(
                MethodId::from_raw(method_index as u32),
                CanonicalName::from(&method.name),
                metadata::decl_meta(method.doc.as_ref(), method.deprecated.as_ref()),
                surface.target,
                callable_decl,
            ))
        })
        .collect()
}

pub struct CallbackMethodSurface<S: SurfaceLower, T> {
    target: T,
    execution: ExecutionDecl<S>,
}

impl<S: SurfaceLower, T> CallbackMethodSurface<S, T> {
    pub fn new(target: T, execution: ExecutionDecl<S>) -> Self {
        Self { target, execution }
    }
}

fn require_callback_receiver(receiver: Receiver) -> Result<(), LowerError> {
    match receiver {
        Receiver::Shared => Ok(()),
        Receiver::None | Receiver::Owned | Receiver::Mutable => Err(LowerError::unsupported_type(
            UnsupportedType::InvalidCallbackReceiver,
        )),
    }
}

fn symbol_owner(owner: callable::CallableOwner) -> SymbolOwner {
    match owner {
        callable::CallableOwner::Record(record) => SymbolOwner::record(record.id.as_str()),
        callable::CallableOwner::Enum(enumeration) => {
            SymbolOwner::enumeration(enumeration.id.as_str())
        }
        callable::CallableOwner::Class(class) => SymbolOwner::class(class.id.as_str()),
        callable::CallableOwner::Trait(source_trait) => {
            SymbolOwner::callback(source_trait.id.as_str())
        }
        callable::CallableOwner::Function | callable::CallableOwner::Closure => {
            unreachable!("free functions and closures do not own methods")
        }
    }
}

fn reject_owned_class_receiver(method: &MethodDef) -> Result<(), LowerError> {
    if matches!(method.receiver, Receiver::Owned) {
        // A class crosses FFI as a handle: an opaque integer or pointer
        // that names a Rust-side instance. The handle's lifetime is
        // managed by reference counts or explicit drops; the foreign
        // side calls `release_class_<class_id>(handle)` to dispose of
        // it.
        //
        // `&self` and `&mut self` are borrows. The method runs with
        // temporary access to the handle's target. The handle stays
        // valid on the foreign side afterward. No lifecycle change.
        //
        // `self` (owned) means the method consumes the instance. After
        // it returns, the handle on the foreign side must be invalid.
        // Calling any other method on it (including the release
        // function) would be use-after-free.
        //
        // The IR does not model handle consumption today. There is no
        // protocol that tells the foreign side "this method invalidated
        // your handle, do not release it." Until that protocol exists
        // (handle poisoning, foreign-language move semantics, or an
        // explicit consumed flag on the method decl), this rejection
        // prevents the unsound case from reaching renderers.
        Err(LowerError::unsupported_type(
            UnsupportedType::OwnedClassReceiver,
        ))
    } else {
        Ok(())
    }
}
