use std::marker::PhantomData;

use crate::wire::{DecodeError, DecodeResult, InvalidWireValue, WireDecode, WireEncode};

/// Static pool metadata for [`InternedString`].
pub trait InternedStringPool {
    /// String values addressable by interned wire id.
    const VALUES: &'static [&'static str];
}

/// A UTF-8 string value that can cross the wire as either a static pool id or
/// dynamic bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternedString<P> {
    repr: InternedStringRepr,
    _pool: PhantomData<P>,
}

/// Runtime representation of an interned string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InternedStringRepr {
    /// Index into the generated static string pool.
    Interned(u32),
    /// Dynamic UTF-8 string fallback for open-set values.
    Dynamic(String),
}

impl<P> InternedString<P> {
    /// Builds an interned value from a trusted static pool id.
    ///
    /// # Safety
    ///
    /// The caller must ensure `id` names a value in `P`'s generated pool.
    pub const unsafe fn from_id_unchecked(id: u32) -> Self {
        Self {
            repr: InternedStringRepr::Interned(id),
            _pool: PhantomData,
        }
    }

    /// Builds a dynamic fallback value.
    pub fn dynamic(value: impl Into<String>) -> Self {
        Self {
            repr: InternedStringRepr::Dynamic(value.into()),
            _pool: PhantomData,
        }
    }

    /// Returns this value's representation.
    pub fn repr(&self) -> &InternedStringRepr {
        &self.repr
    }
}

impl<P: InternedStringPool> InternedString<P> {
    /// Builds an interned value when `value` appears in the static pool,
    /// otherwise stores a dynamic fallback string.
    pub fn from_str(value: &str) -> Self {
        match P::VALUES.iter().position(|candidate| *candidate == value) {
            Some(index) => Self {
                repr: InternedStringRepr::Interned(index as u32),
                _pool: PhantomData,
            },
            None => Self::dynamic(value),
        }
    }
}

impl<P: InternedStringPool> From<&str> for InternedString<P> {
    fn from(value: &str) -> Self {
        Self::from_str(value)
    }
}

impl<P: InternedStringPool> From<String> for InternedString<P> {
    fn from(value: String) -> Self {
        match P::VALUES.iter().position(|candidate| *candidate == value) {
            Some(index) => Self {
                repr: InternedStringRepr::Interned(index as u32),
                _pool: PhantomData,
            },
            None => Self::dynamic(value),
        }
    }
}

impl<P> WireEncode for InternedString<P> {
    fn wire_size(&self) -> usize {
        match &self.repr {
            InternedStringRepr::Interned(_) => 1 + core::mem::size_of::<u32>(),
            InternedStringRepr::Dynamic(value) => 1 + value.wire_size(),
        }
    }

    fn encode_to(&self, buffer: &mut [u8]) -> usize {
        match &self.repr {
            InternedStringRepr::Interned(id) => {
                buffer[0] = 0;
                let written = id.encode_to(&mut buffer[1..]);
                1 + written
            }
            InternedStringRepr::Dynamic(value) => {
                buffer[0] = 1;
                let written = value.encode_to(&mut buffer[1..]);
                1 + written
            }
        }
    }
}

impl<P: InternedStringPool> WireDecode for InternedString<P> {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let tag = *buf.first().ok_or(DecodeError::BufferTooSmall)?;
        match tag {
            0 => {
                let (id, used) = u32::decode_from(&buf[1..])?;
                if (id as usize) >= P::VALUES.len() {
                    return Err(DecodeError::InvalidValue(
                        InvalidWireValue::InternedStringId,
                    ));
                }
                Ok((
                    Self {
                        repr: InternedStringRepr::Interned(id),
                        _pool: PhantomData,
                    },
                    1 + used,
                ))
            }
            1 => {
                let (value, used) = String::decode_from(&buf[1..])?;
                Ok((Self::dynamic(value), 1 + used))
            }
            _ => Err(DecodeError::InvalidValue(
                InvalidWireValue::InternedStringTag,
            )),
        }
    }
}
