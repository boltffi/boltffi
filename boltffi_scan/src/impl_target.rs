use crate::{ScanError, spelling, unsupported};

pub(super) struct Target<'source> {
    path: Option<&'source syn::Path>,
    spelling: String,
}

impl<'source> Target<'source> {
    pub(super) fn scan(item: &'source syn::ItemImpl) -> Self {
        let path = match item.self_ty.as_ref() {
            syn::Type::Path(type_path) if type_path.qself.is_none() => Some(&type_path.path),
            _ => None,
        };
        Self {
            path,
            spelling: spelling::ty(&item.self_ty),
        }
    }

    pub(super) fn class(item: &'source syn::ItemImpl) -> Result<Self, ScanError> {
        let target = Self::scan(item);
        unsupported::generics(&item.generics, &format!("class {}", target.spelling()))?;
        if let Some((path, _)) = &item.trait_ {
            return Err(ScanError::UnsupportedClassImplShape {
                target: target.spelling().to_owned(),
            }
            .at(path));
        }
        Ok(target)
    }

    pub(super) fn path(&self) -> Option<&syn::Path> {
        self.path.filter(|path| {
            path.segments
                .iter()
                .all(|segment| segment.arguments.is_empty())
        })
    }

    pub(super) fn spelling(&self) -> &str {
        &self.spelling
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModulePath, ModuleScope};

    fn parse(source: &str) -> syn::ItemImpl {
        syn::parse_str(source).expect("impl block")
    }

    #[test]
    fn resolves_plain_impl_targets_from_module_context() {
        let module = ModuleScope::new(ModulePath::root("demo").child("engine"), &[]);
        let item = parse("impl Runtime {}");
        let target = Target::scan(&item);
        let expansion = target.path().map(|path| module.expand(path));

        assert_eq!(
            expansion.as_ref().and_then(|path| path.candidate()),
            Some("demo::engine::Runtime")
        );
        assert_eq!(target.spelling(), "Runtime");
    }

    #[test]
    fn rejects_targets_that_would_erase_type_arguments() {
        let item = parse("impl Runtime<u32> {}");
        let target = Target::scan(&item);

        assert!(target.path().is_none());
        assert_eq!(target.spelling(), "Runtime<u32>");
    }

    #[test]
    fn rejects_non_path_targets() {
        let item = parse("impl (Runtime, State) {}");
        let target = Target::scan(&item);

        assert!(target.path().is_none());
        assert_eq!(target.spelling(), "(Runtime, State)");
    }
}
