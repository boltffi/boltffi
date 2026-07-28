use std::fmt;

/// A rejected detached-future return contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    DuplicateFuture,
    DuplicateSend,
    DuplicateStatic,
    MissingSend,
    MissingStatic,
    UnsupportedBound,
    InvalidFuturePath,
    InvalidFutureModifier,
    InvalidFutureOutput,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DuplicateFuture => "the `Future` bound appears more than once",
            Self::DuplicateSend => "the `Send` bound appears more than once",
            Self::DuplicateStatic => "the `'static` bound appears more than once",
            Self::MissingSend => "the `Send` bound is required",
            Self::MissingStatic => "the `'static` bound is required",
            Self::UnsupportedBound => {
                "only `Future<Output = T>`, `Send`, and `'static` bounds are supported"
            }
            Self::InvalidFuturePath => {
                "the future trait must be `Future`, `std::future::Future`, or `core::future::Future`"
            }
            Self::InvalidFutureModifier => {
                "the `Future` bound cannot have modifiers or bound lifetimes"
            }
            Self::InvalidFutureOutput => {
                "`Future` must have exactly one associated type binding, `Output = T`"
            }
        })
    }
}

/// Parses the exact detached-future return shape supported by BoltFFI.
pub fn output(return_type: &syn::ReturnType) -> Result<Option<&syn::Type>, Error> {
    let syn::ReturnType::Type(_, return_type) = return_type else {
        return Ok(None);
    };
    let syn::Type::ImplTrait(impl_trait) = unwrapped(return_type) else {
        return Ok(None);
    };
    if !impl_trait.bounds.iter().any(is_future_bound) {
        return Ok(None);
    }

    let mut future_output = None;
    let mut has_send = false;
    let mut has_static = false;
    impl_trait.bounds.iter().try_for_each(|bound| match bound {
        syn::TypeParamBound::Trait(trait_bound) if is_future_bound(bound) => {
            if future_output.is_some() {
                return Err(Error::DuplicateFuture);
            }
            future_output = Some(parse_future_output(trait_bound)?);
            Ok(())
        }
        syn::TypeParamBound::Trait(trait_bound) if is_send_bound(trait_bound) => {
            if has_send {
                return Err(Error::DuplicateSend);
            }
            has_send = true;
            Ok(())
        }
        syn::TypeParamBound::Lifetime(lifetime) if lifetime.ident == "static" => {
            if has_static {
                return Err(Error::DuplicateStatic);
            }
            has_static = true;
            Ok(())
        }
        _ => Err(Error::UnsupportedBound),
    })?;

    if !has_send {
        return Err(Error::MissingSend);
    }
    if !has_static {
        return Err(Error::MissingStatic);
    }
    Ok(future_output)
}

fn unwrapped(ty: &syn::Type) -> &syn::Type {
    match ty {
        syn::Type::Paren(paren) => unwrapped(&paren.elem),
        syn::Type::Group(group) => unwrapped(&group.elem),
        _ => ty,
    }
}

fn is_future_bound(bound: &syn::TypeParamBound) -> bool {
    matches!(
        bound,
        syn::TypeParamBound::Trait(trait_bound)
            if trait_bound.path.segments.last().is_some_and(|segment| segment.ident == "Future")
    )
}

fn is_send_bound(bound: &syn::TraitBound) -> bool {
    matches!(bound.modifier, syn::TraitBoundModifier::None)
        && bound.lifetimes.is_none()
        && accepted_marker_path(&bound.path, "Send")
}

fn accepted_marker_path(path: &syn::Path, marker: &str) -> bool {
    if path
        .segments
        .iter()
        .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
    {
        return false;
    }
    let segments = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    matches!(segments.as_slice(), [name] if name == marker)
        || matches!(
            segments.as_slice(),
            [root, module, name]
                if matches!(root.as_str(), "std" | "core")
                    && module == "marker"
                    && name == marker
        )
}

fn parse_future_output(bound: &syn::TraitBound) -> Result<&syn::Type, Error> {
    if !matches!(bound.modifier, syn::TraitBoundModifier::None) || bound.lifetimes.is_some() {
        return Err(Error::InvalidFutureModifier);
    }
    let segments = bound
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    if !matches!(segments.as_slice(), [future] if future == "Future")
        && !matches!(
            segments.as_slice(),
            [root, module, future]
                if matches!(root.as_str(), "std" | "core")
                    && module == "future"
                    && future == "Future"
        )
    {
        return Err(Error::InvalidFuturePath);
    }
    if bound
        .path
        .segments
        .iter()
        .take(bound.path.segments.len().saturating_sub(1))
        .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
    {
        return Err(Error::InvalidFuturePath);
    }
    let Some(last) = bound.path.segments.last() else {
        return Err(Error::InvalidFuturePath);
    };
    let syn::PathArguments::AngleBracketed(arguments) = &last.arguments else {
        return Err(Error::InvalidFutureOutput);
    };
    let mut arguments = arguments.args.iter();
    match (arguments.next(), arguments.next()) {
        (Some(syn::GenericArgument::AssocType(output)), None)
            if output.ident == "Output" && output.generics.is_none() =>
        {
            Ok(&output.ty)
        }
        _ => Err(Error::InvalidFutureOutput),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn return_type(source: &str) -> syn::ReturnType {
        syn::parse_str::<syn::Signature>(source)
            .expect("signature")
            .output
    }

    #[test]
    fn accepts_supported_bounds_in_any_order() {
        let first = return_type("fn load() -> impl Future<Output = u32> + Send + 'static");
        let second =
            return_type("fn load() -> impl 'static + Send + core::future::Future<Output = u32>");

        assert!(matches!(output(&first), Ok(Some(syn::Type::Path(_)))));
        assert!(matches!(output(&second), Ok(Some(syn::Type::Path(_)))));
    }

    #[test]
    fn rejects_missing_safety_bounds() {
        let missing_send = return_type("fn load() -> impl Future<Output = u32> + 'static");
        let missing_static = return_type("fn load() -> impl Future<Output = u32> + Send");

        assert!(matches!(output(&missing_send), Err(Error::MissingSend)));
        assert!(matches!(output(&missing_static), Err(Error::MissingStatic)));
    }
}
