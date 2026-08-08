use boltffi_ffi_rules::classification::{self, FieldPrimitive, PassableCategory};
use boltffi_scan::is_boltffi_data_or_error_marker;
use syn::{Attribute, Fields, ItemEnum, ItemStruct, Type};

pub(crate) struct StructDataShape<'a> {
    item_struct: &'a ItemStruct,
}

pub(crate) struct EnumDataShape<'a> {
    item_enum: &'a ItemEnum,
}

struct StructFieldFacts {
    primitives: Vec<FieldPrimitive>,
    total_fields: usize,
}

struct DataItemAttributes<'a> {
    attrs: &'a [Attribute],
}

impl<'a> StructDataShape<'a> {
    pub(crate) fn new(item_struct: &'a ItemStruct) -> Self {
        Self { item_struct }
    }

    pub(crate) fn is_boltffi_data(&self) -> bool {
        DataItemAttributes::new(&self.item_struct.attrs).is_boltffi_data()
    }

    pub(crate) fn passable_category(&self) -> PassableCategory {
        classification::classify_struct(
            self.has_effective_repr_c(),
            &self.classification_primitives(),
        )
    }

    pub(crate) fn is_blittable(&self) -> bool {
        self.passable_category() == PassableCategory::Blittable
    }

    fn has_effective_repr_c(&self) -> bool {
        let data_attributes = DataItemAttributes::new(&self.item_struct.attrs);
        data_attributes.has_repr_c() || !data_attributes.has_any_repr()
    }

    fn classification_primitives(&self) -> Vec<FieldPrimitive> {
        let field_facts = StructFieldFacts::from_fields(&self.item_struct.fields);
        if field_facts.primitives.len() == field_facts.total_fields {
            field_facts.primitives
        } else {
            Vec::new()
        }
    }
}

impl<'a> EnumDataShape<'a> {
    pub(crate) fn new(item_enum: &'a ItemEnum) -> Self {
        Self { item_enum }
    }

    pub(crate) fn is_boltffi_data(&self) -> bool {
        DataItemAttributes::new(&self.item_enum.attrs).is_boltffi_data()
    }

    pub(crate) fn passable_category(&self) -> PassableCategory {
        classification::classify_enum(self.is_c_style(), self.has_effective_integer_repr())
    }

    pub(crate) fn is_c_style(&self) -> bool {
        self.item_enum
            .variants
            .iter()
            .all(|variant| variant.fields.is_empty())
    }

    pub(crate) fn effective_integer_repr(&self) -> syn::Ident {
        DataItemAttributes::new(&self.item_enum.attrs)
            .integer_repr()
            .unwrap_or_else(|| syn::Ident::new("i32", self.item_enum.ident.span()))
    }

    fn has_integer_repr(&self) -> bool {
        DataItemAttributes::new(&self.item_enum.attrs)
            .integer_repr()
            .is_some()
    }

    fn has_effective_integer_repr(&self) -> bool {
        let data_attributes = DataItemAttributes::new(&self.item_enum.attrs);
        self.has_integer_repr() || (self.is_c_style() && !data_attributes.has_any_repr())
    }
}

impl StructFieldFacts {
    fn from_fields(fields: &Fields) -> Self {
        match fields {
            Fields::Named(named_fields) => Self::from_field_iter(named_fields.named.iter()),
            Fields::Unnamed(unnamed_fields) => Self::from_field_iter(unnamed_fields.unnamed.iter()),
            Fields::Unit => Self {
                primitives: Vec::new(),
                total_fields: 0,
            },
        }
    }

    fn from_field_iter<'a>(fields: impl Iterator<Item = &'a syn::Field>) -> Self {
        let field_types = fields.map(|field| &field.ty).collect::<Vec<_>>();
        let primitives = field_types.iter().filter_map(Self::primitive_for).collect();
        Self {
            primitives,
            total_fields: field_types.len(),
        }
    }

    fn primitive_for(rust_type: &&Type) -> Option<FieldPrimitive> {
        match rust_type {
            Type::Path(type_path) => type_path
                .path
                .get_ident()
                .and_then(|ident| FieldPrimitive::from_type_name(&ident.to_string())),
            _ => None,
        }
    }
}

impl<'a> DataItemAttributes<'a> {
    fn new(attrs: &'a [Attribute]) -> Self {
        Self { attrs }
    }

    fn is_boltffi_data(&self) -> bool {
        self.attrs.iter().any(is_boltffi_data_or_error_marker)
    }

    fn has_repr_c(&self) -> bool {
        self.attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("repr"))
            .any(|attribute| {
                attribute
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .ok()
                    .is_some_and(|items| {
                        items.into_iter().any(|item| match item {
                            syn::Meta::Path(path) => path.is_ident("C"),
                            _ => false,
                        })
                    })
            })
    }

    fn integer_repr(&self) -> Option<syn::Ident> {
        self.attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("repr"))
            .find_map(|attribute| {
                attribute
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated,
                    )
                    .ok()
                    .and_then(|idents| {
                        idents.into_iter().find(|ident| {
                            matches!(
                                ident.to_string().as_str(),
                                "i8" | "i16"
                                    | "i32"
                                    | "i64"
                                    | "u8"
                                    | "u16"
                                    | "u32"
                                    | "u64"
                                    | "isize"
                                    | "usize"
                            )
                        })
                    })
            })
    }

    fn has_any_repr(&self) -> bool {
        self.attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("repr"))
    }
}
