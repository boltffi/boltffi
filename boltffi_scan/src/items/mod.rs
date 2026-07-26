pub(super) mod callback;
pub(super) mod class;
pub(super) mod constant;
pub(crate) mod custom_type;
pub(super) mod enumeration;
pub(super) mod function;
pub(super) mod impl_block;
pub(super) mod interned_string_pool;
pub(super) mod record;
pub(super) mod stream;

mod impl_methods;
mod signature;

pub(super) fn misplaced_stream_marker(
    attrs: &[syn::Attribute],
    item: &str,
) -> Result<Option<crate::ScanError>, crate::ScanError> {
    Ok(stream::Attribute::scan(attrs)?.map(|_| stream::Attribute::invalid_placement(item)))
}
