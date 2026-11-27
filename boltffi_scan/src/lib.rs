mod attributes;
mod capture;
mod const_expr;
mod declared_types;
mod error;
mod items;
mod marker;
mod name;
mod repr;
mod spelling;
pub(crate) mod type_expr;
mod unsupported;
mod visibility;

pub use attributes::owned_name;
pub use capture::{
    CapturedItem, CapturedMethods, CapturedPool, SlotSource, capture_class, capture_class_methods,
    capture_constant, capture_custom, capture_custom_ffi, capture_enum, capture_error_enum,
    capture_error_struct, capture_function, capture_interned_string_pool, capture_methods,
    capture_struct, capture_trait,
};
pub use error::{Location, ScanError};
pub use items::class::ClassDeclaration;
pub use marker::CLASS_EXPORT_ARGUMENTS;
pub use repr::scan as scan_repr;
pub use type_expr::primitive_path;
pub use unsupported::{UnsupportedFeature, UnsupportedInfo};
