//! Link symbol names derived without knowing the module path: crate name + item name +
//! a hash of the invocation's span, which `Span::file`/`line`/`column` expose on stable.

use proc_macro::Span;

pub fn export_symbol(item: &str) -> String {
    let span = Span::call_site();
    let crate_name = std::env::var("CARGO_CRATE_NAME").expect("cargo sets CARGO_CRATE_NAME");
    let position = format!("{}:{}:{}", span.file(), span.line(), span.column());

    format!("pim_{crate_name}_{item}_{:08x}", fnv1a(&position))
}

fn fnv1a(input: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for byte in input.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
