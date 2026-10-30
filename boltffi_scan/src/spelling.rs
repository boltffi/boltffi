use quote::ToTokens;

pub(super) fn path(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(segment)
        .collect::<Vec<_>>()
        .join("::")
}

pub(super) fn ty(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Paren(paren) => self::ty(&paren.elem),
        syn::Type::Group(group) => self::ty(&group.elem),
        syn::Type::Path(type_path) => path(&type_path.path),
        syn::Type::ImplTrait(impl_trait) => format!(
            "impl {}",
            impl_trait
                .bounds
                .iter()
                .map(type_param_bound)
                .collect::<Vec<_>>()
                .join(" + ")
        ),
        syn::Type::FnPtr(fn_ptr) => {
            let inputs = fn_ptr
                .inputs
                .iter()
                .map(|input| self::ty(&input.ty))
                .collect::<Vec<_>>()
                .join(", ");
            match &fn_ptr.output {
                syn::ReturnType::Default => format!("fn({inputs})"),
                syn::ReturnType::Type(_, output) => {
                    format!("fn({inputs}) -> {}", self::ty(output))
                }
            }
        }
        syn::Type::Reference(reference) => match reference.mutability {
            Some(_) => format!("&mut {}", self::ty(&reference.elem)),
            None => format!("&{}", self::ty(&reference.elem)),
        },
        syn::Type::Slice(slice) => format!("[{}]", self::ty(&slice.elem)),
        syn::Type::TraitObject(object) => format!(
            "dyn {}",
            object
                .bounds
                .iter()
                .map(type_param_bound)
                .collect::<Vec<_>>()
                .join(" + ")
        ),
        syn::Type::Tuple(tuple) => {
            let rendered = tuple
                .elems
                .iter()
                .map(self::ty)
                .collect::<Vec<_>>()
                .join(", ");
            match tuple.elems.len() {
                1 => format!("({rendered},)"),
                _ => format!("({rendered})"),
            }
        }
        _ => "unrecognized type".to_owned(),
    }
}

pub(super) fn attr(attr: &syn::Attribute) -> String {
    let path = self::path(attr.path());
    match &attr.meta {
        syn::Meta::Path(_) => path,
        syn::Meta::List(list) => format!("{}({})", path, list.tokens),
        syn::Meta::NameValue(_) => path,
    }
}

fn segment(segment: &syn::PathSegment) -> String {
    let ident = segment.ident.to_string();
    match &segment.arguments {
        syn::PathArguments::None => ident,
        syn::PathArguments::AngleBracketed(arguments) => {
            let rendered = arguments
                .args
                .iter()
                .map(generic_argument)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{ident}<{rendered}>")
        }
        syn::PathArguments::Parenthesized(arguments) => {
            let inputs = arguments
                .inputs
                .iter()
                .map(|input| self::ty(&input.ty))
                .collect::<Vec<_>>()
                .join(", ");
            match &arguments.output {
                syn::ReturnType::Default => format!("{ident}({inputs})"),
                syn::ReturnType::Type(_, output) => {
                    format!("{ident}({inputs}) -> {}", self::ty(output))
                }
            }
        }
    }
}

fn generic_argument(argument: &syn::GenericArgument) -> String {
    match argument {
        syn::GenericArgument::Type(ty) => self::ty(ty),
        syn::GenericArgument::Lifetime(lifetime) => lifetime.to_string(),
        syn::GenericArgument::Const(expr) => expr.to_token_stream().to_string(),
        syn::GenericArgument::AssocType(associated) => {
            format!("{} = {}", associated.ident, self::ty(&associated.ty))
        }
        syn::GenericArgument::AssocConst(associated) => {
            format!(
                "{} = {}",
                associated.ident,
                associated.value.to_token_stream()
            )
        }
        syn::GenericArgument::Constraint(constraint) => constraint.ident.to_string(),
        _ => "argument".to_owned(),
    }
}

fn type_param_bound(bound: &syn::TypeParamBound) -> String {
    match bound {
        syn::TypeParamBound::Trait(bound) => path(&bound.path),
        syn::TypeParamBound::Lifetime(lifetime) => lifetime.to_string(),
        _ => "bound".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    fn parse_path(source: &str) -> syn::Path {
        syn::parse_str(source).expect("valid path")
    }

    fn parse_type(source: &str) -> syn::Type {
        syn::parse_str(source).expect("valid type")
    }

    #[test]
    fn joins_path_segments_with_double_colon() {
        assert_eq!(
            super::path(&parse_path("crate::geometry::Point<u32>")),
            "crate::geometry::Point<u32>"
        );
    }

    #[test]
    fn renders_paths_groups_and_references_for_types() {
        assert_eq!(super::ty(&parse_type("Point")), "Point");
        assert_eq!(super::ty(&parse_type("Point<u32>")), "Point<u32>");
        assert_eq!(
            super::ty(&parse_type("Box<dyn Listener + Send>")),
            "Box<dyn Listener + Send>"
        );
        assert_eq!(super::ty(&parse_type("&Point")), "&Point");
        assert_eq!(super::ty(&parse_type("&mut Point")), "&mut Point");
        assert_eq!(super::ty(&parse_type("(Point)")), "Point");
        assert_eq!(super::ty(&parse_type("(Point,)")), "(Point,)");
        assert_eq!(super::ty(&parse_type("(Point, u32)")), "(Point, u32)");
        assert_eq!(super::ty(&parse_type("Array<4>")), "Array<4>");
        assert_eq!(
            super::ty(&parse_type("impl Fn(u32) -> String")),
            "impl Fn(u32) -> String"
        );
        assert_eq!(
            super::ty(&parse_type("fn(u32) -> String")),
            "fn(u32) -> String"
        );
        assert_eq!(super::ty(&parse_type("[u8]")), "[u8]");
    }

    #[test]
    fn renders_unrecognized_types_with_a_stable_label() {
        assert_eq!(super::ty(&parse_type("[u8; 4]")), "unrecognized type");
    }
}
