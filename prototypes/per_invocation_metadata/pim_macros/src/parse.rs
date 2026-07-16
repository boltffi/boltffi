use quote::ToTokens;
use syn::{Fields, GenericArgument, ItemStruct, PathArguments, Type};

const PRIMITIVES: &[&str] = &[
    "bool", "char", "String", "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64",
];

const SHAPES: &[&str] = &["Vec", "Option", "Box", "HashMap", "Result"];

const DIRECT_PRIMITIVES: &[&str] = &[
    "bool", "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64",
];

pub struct Record {
    pub name: String,
    pub direct: bool,
    pub fields: Vec<Field>,
    pub slots: Slots,
}

pub struct Field {
    pub name: String,
    pub ty: TypeNode,
}

pub enum TypeNode {
    Prim(String),
    Shape { name: String, args: Vec<TypeNode> },
    Slot(usize),
}

/// Types the payload cannot name itself; the compiler resolves them at the referencing site.
#[derive(Default)]
pub struct Slots {
    types: Vec<Type>,
    keys: Vec<String>,
}

impl Slots {
    fn intern(&mut self, ty: &Type) -> usize {
        let key = ty.to_token_stream().to_string();
        if let Some(index) = self.keys.iter().position(|known| known == &key) {
            return index;
        }

        self.keys.push(key);
        self.types.push(ty.clone());
        self.types.len() - 1
    }

    pub fn types(&self) -> &[Type] {
        &self.types
    }
}

pub fn record(item: &ItemStruct) -> syn::Result<Record> {
    let Fields::Named(named) = &item.fields else {
        return Err(syn::Error::new_spanned(
            &item.fields,
            "pim: only structs with named fields are supported",
        ));
    };

    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "pim: generic types have no single canonical id",
        ));
    }

    let mut slots = Slots::default();
    let fields = named
        .named
        .iter()
        .map(|field| {
            Ok(Field {
                name: field
                    .ident
                    .as_ref()
                    .expect("named fields carry an identifier")
                    .to_string(),
                ty: classify(&field.ty, &mut slots)?,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(Record {
        name: item.ident.to_string(),
        direct: direct(item, &fields),
        fields,
        slots,
    })
}

/// Conservative: an alias to a primitive downgrades the record to encoded, never the reverse.
fn direct(item: &ItemStruct, fields: &[Field]) -> bool {
    let repr_c = item.attrs.iter().any(|attribute| {
        attribute.path().is_ident("repr")
            && attribute
                .parse_args::<syn::Ident>()
                .is_ok_and(|ident| ident == "C")
    });

    repr_c
        && !fields.is_empty()
        && fields.iter().all(|field| {
            matches!(&field.ty, TypeNode::Prim(name) if DIRECT_PRIMITIVES.contains(&name.as_str()))
        })
}

pub fn classify(ty: &Type, slots: &mut Slots) -> syn::Result<TypeNode> {
    let Type::Path(path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "pim: only path types are supported",
        ));
    };

    if path.qself.is_some() || path.path.segments.len() != 1 {
        return Ok(TypeNode::Slot(slots.intern(ty)));
    }

    let segment = path
        .path
        .segments
        .last()
        .expect("a path has at least one segment");
    let name = segment.ident.to_string();

    match &segment.arguments {
        PathArguments::None if PRIMITIVES.contains(&name.as_str()) => Ok(TypeNode::Prim(name)),
        PathArguments::None => Ok(TypeNode::Slot(slots.intern(ty))),
        PathArguments::AngleBracketed(arguments) if SHAPES.contains(&name.as_str()) => {
            let args = arguments
                .args
                .iter()
                .map(|argument| match argument {
                    GenericArgument::Type(ty) => classify(ty, slots),
                    _ => Err(syn::Error::new_spanned(
                        argument,
                        "pim: only type arguments are supported",
                    )),
                })
                .collect::<syn::Result<Vec<_>>>()?;
            Ok(TypeNode::Shape { name, args })
        }
        _ => Err(syn::Error::new_spanned(ty, "pim: unsupported generic type")),
    }
}
