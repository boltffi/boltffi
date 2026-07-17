use std::collections::{BTreeMap, HashMap};

use crate::safety::PANIC_STATUS;
use crate::status::{FfiStatus, set_last_error};
use crate::types::FfiBuf;
use crate::wire::{WireDecode, WireEncode};

/// A value's ABI at a per-invocation wrapper boundary: itself when direct, an owned
/// [`FfiBuf`] when encoded. One value per parameter, so wrapper arity never depends on
/// knowledge only the referenced type's definition site has.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot cross the FFI boundary by value",
    label = "cannot cross by value",
    note = "annotate `{Self}` with `#[data]`, or use a supported primitive or container"
)]
pub trait FfiCross<Tag>: Sized {
    /// The `extern "C"` type this value crosses as.
    type Ffi;
    /// Converts an owned value into its crossing representation.
    fn lower(self) -> Self::Ffi;
    /// Reconstructs an owned value from its crossing representation.
    fn lift(ffi: Self::Ffi) -> Self;
    /// The value returned beside a panic status; never valid to read.
    fn poisoned() -> Self::Ffi;
}

/// Records a caught wrapper panic: stores the message as the last error and writes
/// [`PANIC_STATUS`] through the out-parameter when one was supplied.
///
/// # Safety
///
/// `status` must be null or valid for writes.
pub unsafe fn note_panic(status: *mut FfiStatus, panic: Box<dyn std::any::Any + Send>) {
    set_last_error(panic_message(panic));
    if !status.is_null() {
        unsafe { *status = PANIC_STATUS };
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    match panic.downcast_ref::<&str>() {
        Some(message) => (*message).to_owned(),
        None => match panic.downcast::<String>() {
            Ok(message) => *message,
            Err(_) => "panic payload is not a string".to_owned(),
        },
    }
}

macro_rules! direct_cross {
    ($($ty:ty => $poisoned:expr),* $(,)?) => {
        $(
            impl<Tag> FfiCross<Tag> for $ty {
                type Ffi = Self;
                fn lower(self) -> Self {
                    self
                }
                fn lift(ffi: Self) -> Self {
                    ffi
                }
                fn poisoned() -> Self {
                    $poisoned
                }
            }
        )*
    };
}

direct_cross!(
    u8 => 0, u16 => 0, u32 => 0, u64 => 0,
    i8 => 0, i16 => 0, i32 => 0, i64 => 0,
    usize => 0, isize => 0,
    f32 => 0.0, f64 => 0.0,
    bool => false,
    () => (),
);

macro_rules! encoded_cross {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<Tag> FfiCross<Tag> for $ty
            where
                Self: WireEncode + WireDecode,
            {
                type Ffi = FfiBuf;

                fn lower(self) -> FfiBuf {
                    FfiBuf::wire_encode(&self)
                }

                fn lift(ffi: FfiBuf) -> Self {
                    crate::wire::decode(unsafe { ffi.as_byte_slice() })
                        .expect("wire decode failed at the FFI boundary")
                }

                fn poisoned() -> FfiBuf {
                    FfiBuf::empty()
                }
            }
        )*
    };
}

encoded_cross!(String);

macro_rules! encoded_cross_generic {
    ($({$($generics:tt)*} $ty:ty),* $(,)?) => {
        $(
            impl<Tag, $($generics)*> FfiCross<Tag> for $ty
            where
                Self: WireEncode + WireDecode,
            {
                type Ffi = FfiBuf;

                fn lower(self) -> FfiBuf {
                    FfiBuf::wire_encode(&self)
                }

                fn lift(ffi: FfiBuf) -> Self {
                    crate::wire::decode(unsafe { ffi.as_byte_slice() })
                        .expect("wire decode failed at the FFI boundary")
                }

                fn poisoned() -> FfiBuf {
                    FfiBuf::empty()
                }
            }
        )*
    };
}

encoded_cross_generic!(
    {T} Vec<T>,
    {T} Option<T>,
    {T} Box<T>,
    {K, V} HashMap<K, V>,
    {K, V} BTreeMap<K, V>,
    {T, E} Result<T, E>,
);

#[cfg(test)]
mod tests {
    use super::*;

    struct ProbeTag;

    extern "C" fn probe_direct(
        value: <f64 as FfiCross<ProbeTag>>::Ffi,
    ) -> <f64 as FfiCross<ProbeTag>>::Ffi {
        let lifted = <f64 as FfiCross<ProbeTag>>::lift(value);
        <f64 as FfiCross<ProbeTag>>::lower(lifted + 1.0)
    }

    extern "C" fn probe_encoded(
        value: <String as FfiCross<ProbeTag>>::Ffi,
    ) -> <String as FfiCross<ProbeTag>>::Ffi {
        let lifted = <String as FfiCross<ProbeTag>>::lift(value);
        <String as FfiCross<ProbeTag>>::lower(format!("{lifted}!"))
    }

    #[test]
    fn extern_signatures_project_through_the_crossing() {
        assert_eq!(
            probe_direct(<f64 as FfiCross<ProbeTag>>::lower(1.5)),
            2.5,
            "direct values cross as themselves"
        );

        let encoded = probe_encoded(<String as FfiCross<ProbeTag>>::lower("hey".to_owned()));
        assert_eq!(
            <String as FfiCross<ProbeTag>>::lift(encoded),
            "hey!",
            "encoded values cross as owned buffers"
        );
    }

    #[test]
    fn containers_round_trip_through_the_wire_encoding() {
        let values = vec![Some("ab".to_owned()), None];
        let lifted =
            <Vec<Option<String>> as FfiCross<ProbeTag>>::lift(<Vec<Option<String>> as FfiCross<
                ProbeTag,
            >>::lower(values.clone()));
        assert_eq!(lifted, values, "containers compose in the byte encoding");
    }

    #[test]
    fn a_noted_panic_sets_the_status_and_last_error() {
        let mut status = FfiStatus::OK;
        unsafe { note_panic(&mut status, Box::new("boom".to_owned())) };
        assert_eq!(
            status, PANIC_STATUS,
            "the out-parameter takes the panic status"
        );
        assert_eq!(
            crate::status::take_last_error().as_deref(),
            Some("boom"),
            "the panic message lands in the last error"
        );
    }
}
