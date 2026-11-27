use boltffi_ast::{CanonicalName, SourceName};

#[cfg(test)]
use boltffi_ast::NamePart;

pub(super) fn source(ident: &syn::Ident) -> SourceName {
    SourceName::new(ident.to_string(), canonical(ident))
}

pub(super) fn canonical(ident: &syn::Ident) -> CanonicalName {
    CanonicalName::from(ident_source(ident).as_str())
}

/// The placeholder id of an item the captured invocation declares.
pub(super) fn self_id(ident: &syn::Ident) -> String {
    format!("$self::{ident}")
}

fn ident_source(ident: &syn::Ident) -> String {
    ident
        .to_string()
        .strip_prefix("r#")
        .map_or_else(|| ident.to_string(), ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(source: &str) -> syn::Ident {
        syn::parse_str(source).expect("valid identifier")
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    #[test]
    fn splits_snake_case_identifier() {
        assert_eq!(
            canonical(&ident("make_handler")),
            name(&["make", "handler"])
        );
        assert_eq!(
            canonical(&ident("make__handler")),
            name(&["make", "handler"])
        );
        assert_eq!(
            canonical(&ident("_make_handler_")),
            name(&["make", "handler"])
        );
    }

    #[test]
    fn splits_acronym_identifier() {
        assert_eq!(canonical(&ident("HTTPRequest")), name(&["http", "request"]));
        assert_eq!(
            canonical(&ident("HTTPServerURL")),
            name(&["http", "server", "url"])
        );
        assert_eq!(canonical(&ident("URL")), CanonicalName::single("url"));
    }

    #[test]
    fn strips_raw_identifier_prefix() {
        assert_eq!(canonical(&ident("r#type")), CanonicalName::single("type"));
        assert_eq!(
            canonical(&ident("r#async_handler")),
            name(&["async", "handler"])
        );
    }

    #[test]
    fn keeps_single_word_identifier_as_one_part() {
        assert_eq!(canonical(&ident("Point")), CanonicalName::single("point"));
    }

    #[test]
    fn keeps_digits_attached_to_their_word_part() {
        assert_eq!(canonical(&ident("Point2D")), name(&["point2", "d"]));
        assert_eq!(canonical(&ident("Vector3")), name(&["vector3"]));
        assert_eq!(canonical(&ident("user2_id")), name(&["user2", "id"]));
    }

    #[test]
    fn source_name_preserves_spelling_and_canonical_projection() {
        let http_request = source(&ident("HTTPRequest"));
        assert_eq!(http_request.spelling(), "HTTPRequest");
        assert_eq!(http_request.canonical(), &name(&["http", "request"]));

        let raw_type = source(&ident("r#type"));
        assert_eq!(raw_type.spelling(), "r#type");
        assert_eq!(raw_type.canonical(), &CanonicalName::single("type"));
    }
}
