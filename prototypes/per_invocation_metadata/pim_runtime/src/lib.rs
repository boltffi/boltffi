//! Record framing shared by the emitting macro and the artifact reader.

mod codec;
mod compose;
mod parse;

pub use codec::{CallStatus, Codec, Encode, RawBuffer, panic_message};
pub use compose::{CAPACITY, Meta, TypeMeta};
pub use parse::{ParseError, RawRecord, records};

/// Canonical identity of a type, baked in by the macro at the definition site.
///
/// `Tag` is the referencing crate's own marker type. Local types get a blanket impl over it;
/// remote types are impl'd against one crate's tag, which is what keeps the orphan rule happy.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no canonical id, so it cannot cross the FFI boundary",
    label = "no canonical id",
    note = "annotate `{Self}` with `#[data]`, or declare it with `custom_type!` if it is foreign"
)]
pub trait TypeInfo<Tag> {
    const MODULE: &'static str;
    const NAME: &'static str;
}

pub const MAGIC: [u8; 8] = *b"PIMMETA1";

pub const MACH_O_SECTION: &str = "__DATA,__pimmeta";
pub const OBJECT_SECTION: &str = ".pimmeta";

pub const MACH_O_SECTION_NAME: &str = "__pimmeta";
pub const OBJECT_SECTION_NAME: &str = ".pimmeta";

pub const fn record_len(module: &str, slots: &[&str], json: &[u8]) -> usize {
    let mut length = MAGIC.len() + size_of::<u64>() + size_of::<u16>() + module.len();
    length += size_of::<u16>();

    let mut index = 0;
    while index < slots.len() {
        length += size_of::<u16>() + slots[index].len();
        index += 1;
    }

    length + size_of::<u32>() + json.len()
}

pub const fn record<const N: usize>(module: &str, slots: &[&str], json: &[u8]) -> [u8; N] {
    let mut bytes = [0u8; N];
    let mut at = write(&mut bytes, 0, &MAGIC);

    let payload = (N - MAGIC.len() - size_of::<u64>()) as u64;
    at = write(&mut bytes, at, &payload.to_le_bytes());
    at = write_str(&mut bytes, at, module);
    at = write(&mut bytes, at, &(slots.len() as u16).to_le_bytes());

    let mut index = 0;
    while index < slots.len() {
        at = write_str(&mut bytes, at, slots[index]);
        index += 1;
    }

    at = write(&mut bytes, at, &(json.len() as u32).to_le_bytes());
    at = write(&mut bytes, at, json);

    assert!(at == N, "record length disagrees with record_len");
    bytes
}

/// Secondary probe: the single-concatenated-id variant of the slot table.
pub const fn id_len(module: &str, name: &str) -> usize {
    module.len() + 2 + name.len()
}

pub const fn join_id<const N: usize>(module: &str, name: &str) -> [u8; N] {
    let mut bytes = [0u8; N];
    let mut at = write(&mut bytes, 0, module.as_bytes());
    at = write(&mut bytes, at, b"::");
    at = write(&mut bytes, at, name.as_bytes());

    assert!(at == N, "id length disagrees with id_len");
    bytes
}

pub const fn str_from(bytes: &'static [u8]) -> &'static str {
    match str::from_utf8(bytes) {
        Ok(value) => value,
        Err(_) => panic!("joined id is not utf-8"),
    }
}

const fn write(bytes: &mut [u8], at: usize, source: &[u8]) -> usize {
    let mut index = 0;
    while index < source.len() {
        bytes[at + index] = source[index];
        index += 1;
    }
    at + source.len()
}

const fn write_str(bytes: &mut [u8], at: usize, value: &str) -> usize {
    let at = write(bytes, at, &(value.len() as u16).to_le_bytes());
    write(bytes, at, value.as_bytes())
}
