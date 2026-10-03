//! Value transport for exported functions: the compiler picks each type's ABI through
//! `Codec::FfiType`, so wrappers never need to know whether a type is direct or encoded.

use std::mem::ManuallyDrop;

/// Heap bytes crossing the boundary. The receiver owns them.
#[repr(C)]
pub struct RawBuffer {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl RawBuffer {
    pub const fn null() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }

    pub fn from_vec(bytes: Vec<u8>) -> Self {
        let mut bytes = ManuallyDrop::new(bytes);
        Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            cap: bytes.capacity(),
        }
    }

    pub fn into_vec(self) -> Vec<u8> {
        unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) }
    }
}

/// A foreign trait implementation crossing into Rust: an opaque foreign handle plus the
/// vtable the generated trait impl dispatches through. The `#[callback]` expansion casts
/// `vtable` to its own repr(C) vtable type.
#[repr(C)]
pub struct CallbackHandle {
    pub data: u64,
    pub vtable: *const std::ffi::c_void,
}

impl CallbackHandle {
    pub const fn null() -> Self {
        Self {
            data: 0,
            vtable: std::ptr::null(),
        }
    }
}

/// Trailing out-parameter on every wrapper: the call's panic channel. The wrapper writes
/// `SUCCESS` on entry; a caught panic flips the code, stores the message, and the wrapper's
/// return value is [`Codec::poisoned`] — never read it.
#[repr(C)]
pub struct CallStatus {
    pub code: u8,
    pub panic_message: RawBuffer,
}

impl CallStatus {
    pub const SUCCESS: u8 = 0;
    pub const PANIC: u8 = 1;

    pub const fn success() -> Self {
        Self {
            code: Self::SUCCESS,
            panic_message: RawBuffer::null(),
        }
    }

    pub fn fail(&mut self, message: String) {
        self.code = Self::PANIC;
        self.panic_message = RawBuffer::from_vec(message.into_bytes());
    }
}

/// Renders a caught panic payload for [`CallStatus::fail`].
pub fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    match panic.downcast_ref::<&str>() {
        Some(message) => (*message).to_owned(),
        None => match panic.downcast::<String>() {
            Ok(message) => *message,
            Err(_) => "panic payload is not a string".to_owned(),
        },
    }
}

/// Byte-level encoding, composable across fields and containers. Bounds on generated impls are
/// lazy (`where` clauses), so a missing impl only errors where a value actually crosses.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no FFI encoding, so its value cannot cross the FFI boundary",
    label = "no FFI encoding",
    note = "annotate `{Self}` with `#[data]` to generate one"
)]
pub trait Encode<Tag>: Sized {
    fn write(&self, out: &mut Vec<u8>);
    fn read(input: &mut &[u8]) -> Self;
}

/// A type's ABI at an exported boundary: itself when direct, a [`RawBuffer`] when encoded.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no FFI codec, so it cannot be a parameter or return of an exported function",
    label = "no FFI codec",
    note = "annotate `{Self}` with `#[data]`, or use a supported primitive or container"
)]
pub trait Codec<Tag>: Sized {
    type FfiType;
    fn lower(self) -> Self::FfiType;
    fn lift(ffi: Self::FfiType) -> Self;
    /// The value returned alongside a [`CallStatus::PANIC`] status. Never valid to read.
    fn poisoned() -> Self::FfiType;
}

fn take<'a>(input: &mut &'a [u8], len: usize) -> &'a [u8] {
    let (bytes, rest) = input.split_at(len);
    *input = rest;
    bytes
}

macro_rules! fixed_width {
    ($($ty:ty),*) => {
        $(
            impl<Tag> Encode<Tag> for $ty {
                fn write(&self, out: &mut Vec<u8>) {
                    out.extend_from_slice(&self.to_le_bytes());
                }

                fn read(input: &mut &[u8]) -> Self {
                    Self::from_le_bytes(
                        take(input, size_of::<Self>())
                            .try_into()
                            .expect("take yields the exact width"),
                    )
                }
            }

            impl<Tag> Codec<Tag> for $ty {
                type FfiType = Self;
                fn lower(self) -> Self {
                    self
                }
                fn lift(ffi: Self) -> Self {
                    ffi
                }
                fn poisoned() -> Self {
                    0 as $ty
                }
            }
        )*
    };
}

fixed_width!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

impl<Tag> Encode<Tag> for bool {
    fn write(&self, out: &mut Vec<u8>) {
        out.push(*self as u8);
    }

    fn read(input: &mut &[u8]) -> Self {
        take(input, 1)[0] != 0
    }
}

impl<Tag> Codec<Tag> for bool {
    type FfiType = bool;
    fn lower(self) -> bool {
        self
    }
    fn lift(ffi: bool) -> bool {
        ffi
    }
    fn poisoned() -> bool {
        false
    }
}

impl<Tag> Encode<Tag> for String {
    fn write(&self, out: &mut Vec<u8>) {
        Encode::<Tag>::write(&(self.len() as u64), out);
        out.extend_from_slice(self.as_bytes());
    }

    fn read(input: &mut &[u8]) -> Self {
        let len = <u64 as Encode<Tag>>::read(input) as usize;
        String::from_utf8(take(input, len).to_vec()).expect("encoded string is utf-8")
    }
}

impl<Tag, T: Encode<Tag>> Encode<Tag> for Vec<T> {
    fn write(&self, out: &mut Vec<u8>) {
        Encode::<Tag>::write(&(self.len() as u64), out);
        for element in self {
            element.write(out);
        }
    }

    fn read(input: &mut &[u8]) -> Self {
        let len = <u64 as Encode<Tag>>::read(input) as usize;
        (0..len).map(|_| T::read(input)).collect()
    }
}

impl<Tag, T: Encode<Tag>> Encode<Tag> for Option<T> {
    fn write(&self, out: &mut Vec<u8>) {
        match self {
            None => out.push(0),
            Some(value) => {
                out.push(1);
                value.write(out);
            }
        }
    }

    fn read(input: &mut &[u8]) -> Self {
        match take(input, 1)[0] {
            0 => None,
            _ => Some(T::read(input)),
        }
    }
}

impl<Tag, T: Encode<Tag>> Encode<Tag> for Box<T> {
    fn write(&self, out: &mut Vec<u8>) {
        (**self).write(out);
    }

    fn read(input: &mut &[u8]) -> Self {
        Box::new(T::read(input))
    }
}

macro_rules! encoded_codec {
    ($($ty:ty),*) => {
        $(
            impl<Tag> Codec<Tag> for $ty
            where
                $ty: Encode<Tag>,
            {
                type FfiType = RawBuffer;

                fn lower(self) -> RawBuffer {
                    let mut bytes = Vec::new();
                    self.write(&mut bytes);
                    RawBuffer::from_vec(bytes)
                }

                fn lift(ffi: RawBuffer) -> Self {
                    let bytes = ffi.into_vec();
                    Self::read(&mut bytes.as_slice())
                }

                fn poisoned() -> RawBuffer {
                    RawBuffer::null()
                }
            }
        )*
    };
}

encoded_codec!(String);

impl<Tag, T: Encode<Tag>> Codec<Tag> for Vec<T> {
    type FfiType = RawBuffer;

    fn lower(self) -> RawBuffer {
        let mut bytes = Vec::new();
        self.write(&mut bytes);
        RawBuffer::from_vec(bytes)
    }

    fn lift(ffi: RawBuffer) -> Self {
        let bytes = ffi.into_vec();
        Self::read(&mut bytes.as_slice())
    }

    fn poisoned() -> RawBuffer {
        RawBuffer::null()
    }
}

impl<Tag, T: Encode<Tag>, E: Encode<Tag>> Codec<Tag> for Result<T, E> {
    type FfiType = RawBuffer;

    fn lower(self) -> RawBuffer {
        let mut bytes = Vec::new();
        match &self {
            Ok(value) => {
                bytes.push(0);
                value.write(&mut bytes);
            }
            Err(error) => {
                bytes.push(1);
                error.write(&mut bytes);
            }
        }
        RawBuffer::from_vec(bytes)
    }

    fn lift(ffi: RawBuffer) -> Self {
        let bytes = ffi.into_vec();
        let input = &mut bytes.as_slice();
        match take(input, 1)[0] {
            0 => Ok(T::read(input)),
            _ => Err(E::read(input)),
        }
    }

    fn poisoned() -> RawBuffer {
        RawBuffer::null()
    }
}

impl<Tag> Codec<Tag> for () {
    type FfiType = ();
    fn lower(self) {}
    fn lift(_: ()) {}
    fn poisoned() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ProbeTag;

    #[repr(C)]
    struct Direct {
        x: f64,
        y: f64,
    }

    impl<Tag> Codec<Tag> for Direct {
        type FfiType = Self;
        fn lower(self) -> Self {
            self
        }
        fn lift(ffi: Self) -> Self {
            ffi
        }
        fn poisoned() -> Self {
            Self { x: 0.0, y: 0.0 }
        }
    }

    extern "C" fn probe_direct(
        value: <Direct as Codec<ProbeTag>>::FfiType,
    ) -> <Direct as Codec<ProbeTag>>::FfiType {
        let lifted = <Direct as Codec<ProbeTag>>::lift(value);
        <Direct as Codec<ProbeTag>>::lower(Direct {
            x: lifted.x + 1.0,
            y: lifted.y,
        })
    }

    extern "C" fn probe_encoded(
        value: <String as Codec<ProbeTag>>::FfiType,
    ) -> <String as Codec<ProbeTag>>::FfiType {
        let lifted = <String as Codec<ProbeTag>>::lift(value);
        <String as Codec<ProbeTag>>::lower(format!("{lifted}!"))
    }

    #[test]
    fn extern_signatures_project_through_the_codec() {
        let direct = probe_direct(<Direct as Codec<ProbeTag>>::lower(Direct {
            x: 1.0,
            y: 2.0,
        }));
        assert_eq!(direct.x, 2.0, "a direct type crosses by value");

        let encoded = probe_encoded(<String as Codec<ProbeTag>>::lower("hey".to_owned()));
        assert_eq!(
            <String as Codec<ProbeTag>>::lift(encoded),
            "hey!",
            "an encoded type crosses as an owned buffer"
        );
    }

    #[test]
    fn round_trips_nested_encodings() {
        let mut bytes = Vec::new();
        Encode::<ProbeTag>::write(&vec![Some("ab".to_owned()), None], &mut bytes);

        let decoded: Vec<Option<String>> = Encode::<ProbeTag>::read(&mut bytes.as_slice());
        assert_eq!(
            decoded,
            vec![Some("ab".to_owned()), None],
            "containers compose in the byte encoding"
        );
    }

    #[test]
    fn round_trips_a_result() {
        let err: Result<f64, String> = Err("division by zero".to_owned());
        let lifted = <Result<f64, String> as Codec<ProbeTag>>::lift(Codec::<ProbeTag>::lower(err));
        assert_eq!(
            lifted,
            Err("division by zero".to_owned()),
            "the error arm survives the buffer"
        );
    }
}
