use boltffi_ast::{ClassDef, ClassId, ClassThreadSafety};

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::type_expr::Scanner;
use crate::{ScanError, attributes, name};

/// The class an `#[export]` struct or enum declares, with no members: its `#[export] impl`
/// blocks add those.
pub(crate) fn scan_declaration(
    declaration: ClassDeclaration<'_>,
    declared_types: &DeclaredTypes,
    thread_safety: ClassThreadSafety,
) -> Result<ClassDef, ScanError> {
    let ident = declaration.ident();
    crate::unsupported::generics(declaration.generics(), &format!("class {ident}"))?;
    let id = ClassId::new(name::self_id(ident));
    let mut class = ClassDef::new(id, name::source(ident));
    let scanner = Scanner::new(declared_types);
    let attrs = Attributes::new(declaration.attrs(), &scanner);
    class.source = attributes::source(declaration.vis());
    class.doc = attrs.doc();
    class.deprecated = attrs.deprecated()?;
    class.user_attrs = attrs.user_attrs();
    class.thread_safety = thread_safety;
    Ok(class)
}

/// The struct or enum whose `#[export]` declares a class.
#[derive(Clone, Copy)]
pub enum ClassDeclaration<'item> {
    Struct(&'item syn::ItemStruct),
    Enum(&'item syn::ItemEnum),
}

impl<'item> ClassDeclaration<'item> {
    /// The declaration `item` makes, when it is a struct or an enum.
    pub fn of(item: &'item syn::Item) -> Option<Self> {
        match item {
            syn::Item::Struct(item) => Some(Self::Struct(item)),
            syn::Item::Enum(item) => Some(Self::Enum(item)),
            _ => None,
        }
    }

    pub fn ident(self) -> &'item syn::Ident {
        match self {
            Self::Struct(item) => &item.ident,
            Self::Enum(item) => &item.ident,
        }
    }

    fn generics(self) -> &'item syn::Generics {
        match self {
            Self::Struct(item) => &item.generics,
            Self::Enum(item) => &item.generics,
        }
    }

    fn vis(self) -> &'item syn::Visibility {
        match self {
            Self::Struct(item) => &item.vis,
            Self::Enum(item) => &item.vis,
        }
    }

    fn attrs(self) -> &'item [syn::Attribute] {
        match self {
            Self::Struct(item) => &item.attrs,
            Self::Enum(item) => &item.attrs,
        }
    }
}
