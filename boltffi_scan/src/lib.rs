mod attributes;
mod capture;
mod cfg;
mod const_expr;
mod declared_types;
mod error;
mod impl_target;
mod input;
mod items;
mod marked;
mod marker;
mod name;
mod package_graph;
mod path;
mod repr;
mod scan;
pub(crate) mod source_tree;
mod spelling;
pub(crate) mod type_expr;
mod unsupported;
mod visibility;

pub use capture::{
    CapturedItem, CapturedMethods, CapturedPool, SlotSource, capture_class,
    capture_class_constants, capture_constant, capture_custom, capture_custom_ffi, capture_enum,
    capture_error_enum, capture_error_struct, capture_function, capture_interned_string_pool,
    capture_methods, capture_streams, capture_struct, capture_trait,
};
pub use cfg::ActiveCfg;
pub use error::ScanError;
pub use input::ScanInput;
pub use scan::{PackageScan, scan, scan_file, scan_package, scan_source};
pub use unsupported::{UnsupportedFeature, UnsupportedInfo};

use path::{ModulePath, ModuleScope};

/// The contract with each id reduced to crate and item path, the way per-invocation
/// capture mints them. Modules are told apart from items by Rust naming convention.
pub fn crate_scoped(contract: boltffi_ast::SourceContract) -> boltffi_ast::SourceContract {
    let prefix = format!("{}::", contract.package.name.replace('-', "_"));
    let mut value = serde_json::to_value(&contract).expect("contract serializes");
    scope_ids(&mut value, &prefix);
    serde_json::from_value(value).expect("contract deserializes")
}

fn scope_ids(value: &mut serde_json::Value, prefix: &str) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(rest) = text.strip_prefix(prefix) {
                let segments = rest.split("::").collect::<Vec<_>>();
                let start = segments
                    .iter()
                    .position(|segment| {
                        segment.starts_with(|first: char| first.is_ascii_uppercase())
                    })
                    .unwrap_or(segments.len() - 1);
                *text = format!("{prefix}{}", segments[start..].join("::"));
            }
        }
        serde_json::Value::Array(items) => {
            items.iter_mut().for_each(|item| scope_ids(item, prefix))
        }
        serde_json::Value::Object(map) => map.values_mut().for_each(|item| scope_ids(item, prefix)),
        _ => {}
    }
}
