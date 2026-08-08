use boltffi_binding::{
    ConstantOwner, DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl,
    ExportedMethodDecl, FieldKey, InitializerDecl, Native, NativeOpaqueFieldExports, NativeSymbol,
    Receive, TypeRef,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::{Expression as CodecExpression, FixedStruct},
        cpython::render::{function, record as record_render},
        name_style::Name,
        render::Package,
        syntax::{Expression, Identifier, TypeAnnotation},
    },
};

use super::{
    AssociatedCallable, ConstantStub, NameScope, constant::DefaultExpression, type_hint::TypeHint,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordClass {
    pub class_name: Identifier,
    pub exception_name: Option<Identifier>,
    pub register_method: Identifier,
    pub fields: Vec<RecordField>,
    pub constants: Vec<ConstantStub>,
    pub wire: RecordWire,
    pub constructors: Vec<AssociatedCallable>,
    pub static_methods: Vec<AssociatedCallable>,
    pub instance_methods: Vec<AssociatedCallable>,
    /// For native opaque records: drop function and per-field property info.
    pub native_opaque: Option<NativeOpaqueRecordInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOpaqueRecordInfo {
    pub drop_fn: Identifier,
    pub props: Vec<NativeOpaquePropertyInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOpaquePropertyInfo {
    pub name: Identifier,
    pub annotation: TypeAnnotation,
    pub optional: bool,
    pub has_fn: Option<Identifier>,
    pub accessor: NativeOpaqueAccessor,
    /// Name of the CPython borrow wrapper function (for String/Bytes fields).
    /// `None` for primitive fields that don't need a wrapper.
    pub python_borrow_fn: Option<Identifier>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOpaqueAccessor {
    Primitive { get_fn: Identifier },
    String { borrow_fn: Identifier },
    Bytes { borrow_fn: Identifier },
}

impl RecordClass {
    pub fn from_direct(record: &DirectRecordDecl<Native>, package: &Package) -> Result<Self> {
        let c_record =
            package
                .bridge
                .source_direct_record(record.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "direct record package without C typedef",
                })?;
        let symbols = record_render::Symbols::from_direct(record, c_record)?;
        Ok(Self {
            class_name: symbols.class_name().clone(),
            exception_name: record
                .is_error_payload()
                .then(|| package.exception_name(symbols.class_name()))
                .transpose()?,
            register_method: symbols.register_method().clone(),
            fields: record
                .fields()
                .iter()
                .map(|field| RecordField::from_direct(field, package))
                .collect::<Result<Vec<_>>>()?,
            constants: package.constants_for_owner(ConstantOwner::Record(record.id()))?,
            wire: RecordWire::Fixed(FixedStruct::from_layout(
                symbols.class_name(),
                record.fields(),
                record.layout(),
            )?),
            native_opaque: None,
            constructors: Self::constructors(record.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(record.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(record.methods(), &symbols, package)?,
        })
    }

    pub fn from_native_opaque(
        record: &EncodedRecordDecl<Native>,
        package: &Package,
    ) -> Result<Self> {
        use crate::target::python::cpython::render::record as record_render;
        let symbols = record_render::Symbols::from_native_opaque(record)?;
        let exports = record
            .native_opaque_exports()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "native opaque record without helper exports",
            })?;
        let stem = symbols.stem().to_owned();
        // Use the wrapper name (not raw C symbol) so package.py calls a registered method.
        let drop_fn = Identifier::parse(format!("boltffi_python_opaque_drop_{stem}"))?;
        let props = record
            .fields()
            .iter()
            .zip(exports.fields())
            .map(|(field, fexp)| NativeOpaquePropertyInfo::from_field(field, fexp, &stem, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            class_name: symbols.class_name().clone(),
            exception_name: None,
            register_method: symbols.register_method().clone(),
            fields: Vec::new(),
            constants: Vec::new(),
            wire: RecordWire::Fields(Vec::new()),
            native_opaque: Some(NativeOpaqueRecordInfo { drop_fn, props }),
            constructors: Vec::new(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
        })
    }

    pub fn from_encoded(record: &EncodedRecordDecl<Native>, package: &Package) -> Result<Self> {
        let symbols = record_render::Symbols::from_encoded(record)?;
        let fields = record
            .fields()
            .iter()
            .map(|field| RecordField::from_encoded(field, package))
            .collect::<Result<Vec<_>>>()?;
        let wire_fields = record
            .fields()
            .iter()
            .map(|field| EncodedRecordField::from_field(field, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            class_name: symbols.class_name().clone(),
            exception_name: record
                .is_error_payload()
                .then(|| package.exception_name(symbols.class_name()))
                .transpose()?,
            register_method: symbols.register_method().clone(),
            fields,
            constants: package.constants_for_owner(ConstantOwner::Record(record.id()))?,
            wire: RecordWire::Fields(wire_fields),
            native_opaque: None,
            constructors: Self::constructors(record.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(record.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(record.methods(), &symbols, package)?,
        })
    }

    pub fn is_native_opaque(&self) -> bool {
        self.native_opaque.is_some()
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_async_helpers)
    }

    pub fn uses_sequence_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_sequence_annotations)
    }

    pub fn uses_callable_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_callable_annotations)
    }

    pub fn validate_names(&self) -> Result<()> {
        NameScope::new(format!("record `{}`", self.class_name))
            .insert_all(self.fields.iter().map(RecordField::field_name))
            .and_then(|scope| {
                scope.insert_all(self.constants.iter().map(ConstantStub::member_name))
            })
            .and_then(|scope| {
                scope.insert_all(self.callables().map(AssociatedCallable::member_name))
            })
            .map(|_| ())?;
        RecordField::validate_default_order(&self.fields, "record default before required field")?;
        self.callables()
            .try_for_each(|callable| callable.validate_names(&self.class_name))
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.to_string(),
            format!("record `{}`", self.class_name),
        )
    }

    pub fn exception_top_level_name(&self) -> Option<(String, String)> {
        self.exception_name.as_ref().map(|exception_name| {
            (
                exception_name.to_string(),
                format!("record error `{}`", exception_name),
            )
        })
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.constructors
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    fn constructors(
        initializers: &[InitializerDecl<Native>],
        symbols: &record_render::Symbols,
        package: &Package,
    ) -> Result<Vec<AssociatedCallable>> {
        initializers
            .iter()
            .filter(|initializer| function::Function::can_render(initializer.callable()))
            .map(|initializer| {
                AssociatedCallable::from_value_initializer(
                    initializer,
                    symbols.initializer(initializer.name())?,
                    package,
                )
            })
            .collect()
    }

    fn static_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record_render::Symbols,
        package: &Package,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_none()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name())?,
                    None,
                    None,
                    package,
                )
            })
            .collect()
    }

    fn instance_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record_render::Symbols,
        package: &Package,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_some()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name())?,
                    Some(Expression::identifier(Identifier::parse("self")?)),
                    method
                        .callable()
                        .receiver()
                        .filter(|receiver| matches!(receiver, Receive::ByMutRef))
                        .map(|_| TypeAnnotation::identifier(symbols.class_name().clone())),
                    package,
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordField {
    pub name: Identifier,
    pub annotation: TypeAnnotation,
    pub default: Option<Expression>,
}

impl RecordField {
    pub fn from_encoded(field: &EncodedFieldDecl, package: &Package) -> Result<Self> {
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: TypeHint::from_type_ref(field.ty(), package)?.into_annotation(),
            default: field
                .meta()
                .default()
                .map(|value| DefaultExpression::new(value, package))
                .transpose()?
                .map(DefaultExpression::into_expression),
        })
    }

    pub fn field_name(&self) -> (String, String) {
        (self.name.to_string(), format!("field `{}`", self.name))
    }

    pub fn validate_default_order(fields: &[Self], shape: &'static str) -> Result<()> {
        match fields
            .iter()
            .scan(false, |default_seen, field| {
                let required_after_default = *default_seen && field.default.is_none();
                *default_seen |= field.default.is_some();
                Some(required_after_default)
            })
            .any(|required_after_default| required_after_default)
        {
            true => Err(Error::UnsupportedTarget {
                target: "python",
                shape,
            }),
            false => Ok(()),
        }
    }

    pub fn name(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).function(),
            FieldKey::Position(position) => Name::position_field(*position),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record field annotation",
            }),
        }
    }

    fn from_direct(field: &DirectFieldDecl, package: &Package) -> Result<Self> {
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: TypeHint::from_primitive(field.ty().primitive())?.into_annotation(),
            default: field
                .meta()
                .default()
                .map(|value| DefaultExpression::new(value, package))
                .transpose()?
                .map(DefaultExpression::into_expression),
        })
    }
}

/// How a record class moves through wire bytes.
///
/// The variants mirror the record declaration split: a direct record packs
/// its padded memory image through one precompiled `struct.Struct`, and an
/// encoded record walks its fields through per-field codec expressions.
/// Rendering matches on the variant, so a record cannot silently fall off
/// its lane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordWire {
    Fixed(FixedStruct),
    Fields(Vec<EncodedRecordField>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRecordField {
    pub name: Identifier,
    pub encode: Expression,
    pub decode: Expression,
}

impl EncodedRecordField {
    pub fn from_field(field: &EncodedFieldDecl, package: &Package) -> Result<Self> {
        let name = RecordField::name(field.key())?;
        Ok(Self {
            encode: CodecExpression::write_record_field(field.write(), package)?.into_expression(),
            decode: CodecExpression::read(field.read(), package)?.into_expression(),
            name,
        })
    }
}

impl NativeOpaquePropertyInfo {
    /// Derives the C extension wrapper function name for a borrow field.
    /// e.g. stem="snapshot" field_name="title" → "boltffi_python_borrow_opaque_snapshot_title"
    pub fn borrow_wrapper_c_name(stem: &str, field_name: &str) -> Result<Identifier> {
        Identifier::parse(format!("boltffi_python_borrow_opaque_{stem}_{field_name}"))
    }

    fn from_field(
        field: &EncodedFieldDecl,
        exports: &NativeOpaqueFieldExports,
        stem: &str,
        _package: &Package,
    ) -> Result<Self> {
        use crate::target::python::render::type_hint::TypeHint;
        let key = field.key();
        let name = RecordField::name(key)?;
        let (ty_inner, optional) = match field.ty() {
            TypeRef::Optional(inner) => (inner.as_ref(), true),
            ty => (ty, false),
        };
        // Use CPython wrapper names so package.py calls the registered C extension
        // methods, never raw Rust symbols.  The C wrappers load each symbol at runtime
        // via the loaded-function storage pointer; package.py only ever calls the
        // registered wrapper entries, which are set up in native_opaque_record.c.
        let field_name_str = name.as_str().to_owned();
        let has_fn = if optional {
            // Fail rendering if the has export is absent: the bridge contract must
            // provide a presence helper for every optional field.
            exports.has().ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "optional native opaque field without has export",
            })?;
            Some(Identifier::parse(format!(
                "boltffi_python_opaque_has_{stem}_{field_name_str}"
            ))?)
        } else {
            None
        };
        let annotation = TypeHint::from_type_ref(field.ty(), _package)?.into_annotation();
        let accessor = match ty_inner {
            TypeRef::Primitive(_) => {
                // Fail rendering if the get export is absent: the bridge contract
                // must supply a getter for every non-optional primitive field.
                exports.get().ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "primitive native opaque field without get export",
                })?;
                NativeOpaqueAccessor::Primitive {
                    get_fn: Identifier::parse(format!(
                        "boltffi_python_opaque_get_{stem}_{field_name_str}"
                    ))?,
                }
            }
            TypeRef::String => {
                let borrow_sym = exports
                    .borrow()
                    .ok_or(Error::UnsupportedTarget {
                        target: "python",
                        shape: "string native opaque field without borrow export",
                    })?
                    .name()
                    .as_str();
                NativeOpaqueAccessor::String {
                    borrow_fn: Identifier::parse(borrow_sym)?,
                }
            }
            TypeRef::Bytes => {
                let borrow_sym = exports
                    .borrow()
                    .ok_or(Error::UnsupportedTarget {
                        target: "python",
                        shape: "bytes native opaque field without borrow export",
                    })?
                    .name()
                    .as_str();
                NativeOpaqueAccessor::Bytes {
                    borrow_fn: Identifier::parse(borrow_sym)?,
                }
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unsupported native opaque field type",
                });
            }
        };
        // Compute the Python borrow wrapper name for String/Bytes fields.
        let python_borrow_fn = match &accessor {
            NativeOpaqueAccessor::String { .. } | NativeOpaqueAccessor::Bytes { .. } => {
                Some(Self::borrow_wrapper_c_name(stem, &field_name_str)?)
            }
            NativeOpaqueAccessor::Primitive { .. } => None,
        };
        Ok(Self {
            name,
            annotation,
            optional,
            has_fn,
            accessor,
            python_borrow_fn,
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn annotation(&self) -> &TypeAnnotation {
        &self.annotation
    }

    pub fn optional(&self) -> bool {
        self.optional
    }

    pub fn has_fn(&self) -> Option<&Identifier> {
        self.has_fn.as_ref()
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self.accessor, NativeOpaqueAccessor::Primitive { .. })
    }

    pub fn is_string(&self) -> bool {
        matches!(self.accessor, NativeOpaqueAccessor::String { .. })
    }

    #[allow(dead_code)]
    pub fn is_bytes(&self) -> bool {
        matches!(self.accessor, NativeOpaqueAccessor::Bytes { .. })
    }

    pub fn get_fn(&self) -> Option<&Identifier> {
        match &self.accessor {
            NativeOpaqueAccessor::Primitive { get_fn } => Some(get_fn),
            _ => None,
        }
    }

    pub fn python_borrow_fn(&self) -> Option<&Identifier> {
        self.python_borrow_fn.as_ref()
    }

    /// Returns the raw Rust C borrow symbol (used internally for wrapper generation).
    #[allow(dead_code)]
    pub fn borrow_fn(&self) -> Option<&Identifier> {
        match &self.accessor {
            NativeOpaqueAccessor::String { borrow_fn }
            | NativeOpaqueAccessor::Bytes { borrow_fn } => Some(borrow_fn),
            _ => None,
        }
    }
}

impl NativeOpaqueRecordInfo {
    pub fn drop_fn(&self) -> &Identifier {
        &self.drop_fn
    }

    pub fn props(&self) -> &[NativeOpaquePropertyInfo] {
        &self.props
    }
}
