use boltffi_binding::{
    CStyleEnumDecl, CanonicalName, DataEnumDecl, DataVariantDecl, DataVariantPayload,
    EncodedFieldDecl, ExportedMethodDecl, InitializerDecl, Native, NativeSymbol, Receive,
};

use crate::{
    core::{Error, Result},
    target::python::{
        cpython::render::{enumeration as enumeration_render, function},
        name_style::Name,
        syntax::{CallExpression, Expression, Identifier, TypeAnnotation},
    },
};

use super::{
    AssociatedCallable, NameScope, Package, record::EncodedRecordField, record::RecordField,
};

pub enum VariantStyle {
    CStyle,
    Data,
}

impl VariantStyle {
    pub fn expression(
        self,
        enum_name: &CanonicalName,
        variant_name: &CanonicalName,
    ) -> Result<Expression> {
        match self {
            Self::CStyle => Ok(Expression::attribute(
                Expression::identifier(Identifier::parse(Name::new(enum_name).class())?),
                Identifier::parse(Name::new(variant_name).enum_member())?,
            )),
            Self::Data => Ok(Expression::call(CallExpression::new(
                Expression::identifier(Identifier::parse(format!(
                    "{}{}",
                    Name::new(enum_name).class(),
                    Name::new(variant_name).class()
                ))?),
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumClass {
    pub class_name: Identifier,
    pub exception_name: Option<Identifier>,
    pub register_method: Identifier,
    pub variants: Vec<EnumVariant>,
    pub wire: Option<DataEnumWire>,
    pub constructors: Vec<AssociatedCallable>,
    pub static_methods: Vec<AssociatedCallable>,
    pub instance_methods: Vec<AssociatedCallable>,
}

impl EnumClass {
    pub fn from_c_style(enumeration: &CStyleEnumDecl<Native>, package: &Package) -> Result<Self> {
        let class = enumeration_render::PythonClass::from_c_style(enumeration, package.bridge)?;
        let c_enum = package.bridge.source_c_style_enum(enumeration.id()).ok_or(
            Error::UnsupportedTarget {
                target: "python",
                shape: "c-style enum package without C typedef",
            },
        )?;
        let symbols = enumeration_render::Symbols::from_c_style(enumeration, c_enum)?;
        Ok(Self {
            class_name: class.class_name().clone(),
            exception_name: enumeration
                .is_error_payload()
                .then(|| package.exception_name(class.class_name()))
                .transpose()?,
            register_method: class.register_method().clone(),
            variants: class
                .variants()
                .iter()
                .map(EnumVariant::from_variant)
                .collect::<Result<Vec<_>>>()?,
            wire: None,
            constructors: Self::constructors(enumeration.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(enumeration.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(enumeration.methods(), &symbols, package)?,
        })
    }

    pub fn from_data(enumeration: &DataEnumDecl<Native>, package: &Package) -> Result<Self> {
        let symbols = enumeration_render::Symbols::from_data(enumeration)?;
        let class_name = symbols.class_name().clone();
        Ok(Self {
            class_name: class_name.clone(),
            exception_name: enumeration
                .is_error_payload()
                .then(|| package.exception_name(&class_name))
                .transpose()?,
            register_method: symbols.register_method().clone(),
            variants: Vec::new(),
            wire: Some(DataEnumWire {
                variants: enumeration
                    .variants()
                    .iter()
                    .map(|variant| DataEnumVariant::from_variant(variant, &class_name, package))
                    .collect::<Result<Vec<_>>>()?,
            }),
            constructors: Self::constructors(enumeration.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(enumeration.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(enumeration.methods(), &symbols, package)?,
        })
    }
}

impl EnumClass {
    pub fn has_wire(&self) -> bool {
        self.wire.is_some()
    }

    pub fn uses_wire_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_wire_helpers)
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
        let scope = match self.is_int_enum() {
            true => NameScope::new(format!("enum `{}`", self.class_name))
                .insert("name", "reserved IntEnum property `name`")?
                .insert("value", "reserved IntEnum property `value`")?,
            false => NameScope::new(format!("enum `{}`", self.class_name)),
        };
        scope
            .insert_all(self.variants.iter().map(EnumVariant::member_name))
            .and_then(|scope| {
                scope.insert_all(self.callables().map(AssociatedCallable::member_name))
            })
            .map(|_| ())?;
        self.callables()
            .try_for_each(|callable| callable.validate_names(&self.class_name))?;
        self.wire
            .iter()
            .flat_map(|wire| wire.variants.iter())
            .try_for_each(DataEnumVariant::validate_names)
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.to_string(),
            format!("enum `{}`", self.class_name),
        )
    }

    pub fn exception_top_level_name(&self) -> Option<(String, String)> {
        self.exception_name.as_ref().map(|exception_name| {
            (
                exception_name.to_string(),
                format!("enum error `{}`", exception_name),
            )
        })
    }

    pub fn data_variant_names(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.wire
            .iter()
            .flat_map(|wire| wire.variants.iter())
            .map(DataEnumVariant::top_level_name)
    }

    pub fn is_int_enum(&self) -> bool {
        self.wire.is_none()
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.constructors
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    fn constructors(
        initializers: &[InitializerDecl<Native>],
        symbols: &enumeration_render::Symbols,
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
        symbols: &enumeration_render::Symbols,
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
        symbols: &enumeration_render::Symbols,
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
pub struct EnumVariant {
    pub name: Identifier,
    pub value: i128,
}

impl EnumVariant {
    fn from_variant(variant: &enumeration_render::PythonVariant) -> Result<Self> {
        Ok(Self {
            name: variant.name().clone(),
            value: variant.value(),
        })
    }

    fn member_name(&self) -> (String, String) {
        (
            self.name.to_string(),
            format!("enum member `{}`", self.name),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataEnumWire {
    pub variants: Vec<DataEnumVariant>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataEnumVariant {
    pub class_name: Identifier,
    pub tag: u32,
    pub fields: Vec<RecordField>,
    pub wire_fields: Vec<EncodedRecordField>,
}

impl DataEnumVariant {
    pub fn has_fields(&self) -> bool {
        !self.fields.is_empty()
    }

    fn from_variant(
        variant: &DataVariantDecl,
        enum_class_name: &Identifier,
        package: &Package,
    ) -> Result<Self> {
        let fields = Self::payload_fields(variant.payload())?;
        Ok(Self {
            class_name: Identifier::parse(format!(
                "{}{}",
                enum_class_name,
                Name::new(variant.name()).class()
            ))?,
            tag: variant.tag().get(),
            fields: fields
                .iter()
                .map(|field| RecordField::from_encoded(field, package))
                .collect::<Result<Vec<_>>>()?,
            wire_fields: fields
                .iter()
                .map(|field| EncodedRecordField::from_field(field, package))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn validate_names(&self) -> Result<()> {
        NameScope::new(format!("data enum variant `{}`", self.class_name))
            .insert_all(self.fields.iter().map(RecordField::field_name))
            .map(|_| ())?;
        RecordField::validate_default_order(
            &self.fields,
            "data enum payload default before required field",
        )
    }

    fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.to_string(),
            format!("data enum variant `{}`", self.class_name),
        )
    }

    fn payload_fields(payload: &DataVariantPayload) -> Result<&[EncodedFieldDecl]> {
        Ok(match payload {
            DataVariantPayload::Unit => &[],
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => fields,
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown data enum payload",
                });
            }
        })
    }
}
