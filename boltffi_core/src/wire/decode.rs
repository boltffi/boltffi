use crate::wire::encode::{WireEncode, WireEncodingKind};
use crate::wire::temporal::{DurationWireValue, EpochTimestampWireValue};

#[cfg(feature = "chrono")]
use chrono::{DateTime, Utc};

use std::{
    collections::{BTreeMap, HashMap},
    hash::Hash,
    mem::{ManuallyDrop, MaybeUninit},
    time::{Duration, SystemTime},
};

#[cfg(feature = "uuid")]
use uuid::Uuid;

#[cfg(feature = "url")]
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidWireValue {
    Bool,
    OptionTag,
    ResultTag,
    EnumTag,
    InternedStringId,
    InternedStringTag,
    TemporalNanoseconds,
    Url,
    DateTimeUtc,
    CustomConversion,
    DuplicateMapKey,
}

impl std::fmt::Display for InvalidWireValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool => write!(formatter, "Bool"),
            Self::OptionTag => write!(formatter, "OptionTag"),
            Self::ResultTag => write!(formatter, "ResultTag"),
            Self::EnumTag => write!(formatter, "EnumTag"),
            Self::InternedStringId => write!(formatter, "InternedStringId"),
            Self::InternedStringTag => write!(formatter, "InternedStringTag"),
            Self::TemporalNanoseconds => write!(formatter, "TemporalNanoseconds"),
            Self::Url => write!(formatter, "Url"),
            Self::DateTimeUtc => write!(formatter, "DateTimeUtc"),
            Self::CustomConversion => write!(formatter, "CustomConversion"),
            Self::DuplicateMapKey => write!(formatter, "DuplicateMapKey"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    BufferTooSmall,
    InvalidValue(InvalidWireValue),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooSmall => write!(formatter, "BufferTooSmall"),
            Self::InvalidValue(invalid_value) => write!(formatter, "InvalidValue({invalid_value})"),
        }
    }
}

pub type DecodeResult<T> = Result<(T, usize), DecodeError>;

pub trait WireDecode: Sized {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self>;
}

struct WireReader<'buffer> {
    buffer: &'buffer [u8],
    offset: usize,
}

impl<'buffer> WireReader<'buffer> {
    #[inline]
    fn new(buffer: &'buffer [u8]) -> Self {
        Self { buffer, offset: 0 }
    }

    #[inline]
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        self.read_exact(N)?
            .try_into()
            .map_err(|_| DecodeError::BufferTooSmall)
    }

    #[inline]
    fn read_byte(&mut self) -> Result<u8, DecodeError> {
        Ok(self.read_exact(1)?[0])
    }

    #[inline]
    fn read_exact(&mut self, byte_count: usize) -> Result<&'buffer [u8], DecodeError> {
        let start = self.offset;
        let end = start + byte_count;
        let bytes = self
            .buffer
            .get(start..end)
            .ok_or(DecodeError::BufferTooSmall)?;
        self.offset = end;
        Ok(bytes)
    }

    #[inline]
    fn read_value<T: WireDecode>(&mut self) -> Result<T, DecodeError> {
        let (value, used) = T::decode_from(
            self.buffer
                .get(self.offset..)
                .ok_or(DecodeError::BufferTooSmall)?,
        )?;
        self.offset += used;
        Ok(value)
    }

    #[inline]
    fn read_length_prefixed_bytes(&mut self) -> Result<&'buffer [u8], DecodeError> {
        let byte_count = self.read_value::<u32>()? as usize;
        self.read_exact(byte_count)
    }

    #[inline]
    fn finish<T>(self, value: T) -> DecodeResult<T> {
        Ok((value, self.offset))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoolWireValue {
    False,
    True,
}

impl TryFrom<u8> for BoolWireValue {
    type Error = DecodeError;

    fn try_from(tag: u8) -> Result<Self, Self::Error> {
        match tag {
            0 => Ok(Self::False),
            1 => Ok(Self::True),
            _ => Err(DecodeError::InvalidValue(InvalidWireValue::Bool)),
        }
    }
}

impl BoolWireValue {
    fn into_bool(self) -> bool {
        matches!(self, Self::True)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptionWireTag {
    None,
    Some,
}

impl TryFrom<u8> for OptionWireTag {
    type Error = DecodeError;

    fn try_from(tag: u8) -> Result<Self, Self::Error> {
        match tag {
            0 => Ok(Self::None),
            1 => Ok(Self::Some),
            _ => Err(DecodeError::InvalidValue(InvalidWireValue::OptionTag)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultWireTag {
    Ok,
    Err,
}

impl TryFrom<u8> for ResultWireTag {
    type Error = DecodeError;

    fn try_from(tag: u8) -> Result<Self, Self::Error> {
        match tag {
            0 => Ok(Self::Ok),
            1 => Ok(Self::Err),
            _ => Err(DecodeError::InvalidValue(InvalidWireValue::ResultTag)),
        }
    }
}

macro_rules! impl_wire_decode_primitive {
    ($($ty:ty),*) => {
        $(
            impl WireDecode for $ty {
                #[inline]
                fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
                    let mut reader = WireReader::new(buf);
                    let bytes = reader.read_array::<{ core::mem::size_of::<$ty>() }>()?;
                    reader.finish(<$ty>::from_le_bytes(bytes))
                }
            }
        )*
    };
}

impl_wire_decode_primitive!(i8, i16, i32, i64, u8, u16, u32, u64, f32, f64);

impl WireDecode for bool {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let value = BoolWireValue::try_from(reader.read_byte()?)?.into_bool();
        reader.finish(value)
    }
}

impl WireDecode for isize {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let value = i64::from_le_bytes(reader.read_array::<8>()?) as isize;
        reader.finish(value)
    }
}

impl WireDecode for usize {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let value = u64::from_le_bytes(reader.read_array::<8>()?) as usize;
        reader.finish(value)
    }
}

impl WireDecode for String {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let string_bytes = reader.read_length_prefixed_bytes()?;
        let string = unsafe { core::str::from_utf8_unchecked(string_bytes) }.to_owned();
        reader.finish(string)
    }
}

impl WireDecode for Duration {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let wire_value = DurationWireValue {
            seconds: reader.read_value::<u64>()?,
            nanos: reader.read_value::<u32>()?,
        };
        let duration = wire_value.into_duration().ok_or(DecodeError::InvalidValue(
            InvalidWireValue::TemporalNanoseconds,
        ))?;
        reader.finish(duration)
    }
}

impl WireDecode for SystemTime {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let wire_value = EpochTimestampWireValue {
            seconds: reader.read_value::<i64>()?,
            nanos: reader.read_value::<u32>()?,
        };
        let system_time = wire_value
            .into_system_time()
            .ok_or(DecodeError::InvalidValue(
                InvalidWireValue::TemporalNanoseconds,
            ))?;
        reader.finish(system_time)
    }
}

#[cfg(feature = "uuid")]
impl WireDecode for Uuid {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let hi = reader.read_value::<u64>()?;
        let lo = reader.read_value::<u64>()?;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&hi.to_be_bytes());
        bytes[8..].copy_from_slice(&lo.to_be_bytes());
        reader.finish(Uuid::from_bytes(bytes))
    }
}

#[cfg(feature = "url")]
impl WireDecode for Url {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let string = reader.read_value::<String>()?;
        let url =
            Url::parse(&string).map_err(|_| DecodeError::InvalidValue(InvalidWireValue::Url))?;
        reader.finish(url)
    }
}

#[cfg(feature = "chrono")]
impl WireDecode for DateTime<Utc> {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let wire_value = EpochTimestampWireValue {
            seconds: reader.read_value::<i64>()?,
            nanos: reader.read_value::<u32>()?,
        };
        let date_time = wire_value
            .into_date_time_utc()
            .ok_or(DecodeError::InvalidValue(InvalidWireValue::DateTimeUtc))?;
        reader.finish(date_time)
    }
}

impl WireDecode for () {
    #[inline]
    fn decode_from(_buf: &[u8]) -> DecodeResult<Self> {
        Ok(((), 0))
    }
}

impl<T: WireDecode> WireDecode for Option<T> {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        match OptionWireTag::try_from(reader.read_byte()?)? {
            OptionWireTag::None => reader.finish(None),
            OptionWireTag::Some => {
                let value = reader.read_value::<T>()?;
                reader.finish(Some(value))
            }
        }
    }
}

impl<T: WireDecode, E: WireDecode> WireDecode for Result<T, E> {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        match ResultWireTag::try_from(reader.read_byte()?)? {
            ResultWireTag::Ok => {
                let value = reader.read_value::<T>()?;
                reader.finish(Ok(value))
            }
            ResultWireTag::Err => {
                let value = reader.read_value::<E>()?;
                reader.finish(Err(value))
            }
        }
    }
}

impl<T: WireDecode> WireDecode for Box<T> {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let (value, consumed) = T::decode_from(buf)?;
        Ok((Box::new(value), consumed))
    }
}

impl<T: WireDecode + WireEncode> WireDecode for Vec<T> {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let count = reader.read_value::<u32>()? as usize;

        if count == 0 {
            return reader.finish(Vec::new());
        }

        match T::ENCODING_KIND {
            WireEncodingKind::Blittable => {
                let element_size = core::mem::size_of::<T>();
                let data_size = count * element_size;
                let element_bytes = reader.read_exact(data_size)?;
                let mut result = Vec::<MaybeUninit<T>>::with_capacity(count);
                unsafe {
                    result.set_len(count);
                    core::ptr::copy_nonoverlapping(
                        element_bytes.as_ptr(),
                        result.as_mut_ptr() as *mut u8,
                        data_size,
                    );
                    let result = ManuallyDrop::new(result);
                    let initialized_values =
                        Vec::from_raw_parts(result.as_ptr() as *mut T, count, result.capacity());
                    return reader.finish(initialized_values);
                }
            }
            WireEncodingKind::General => {}
        }

        let values = (0..count).try_fold(Vec::with_capacity(count), |mut values, _| {
            values.push(reader.read_value::<T>()?);
            Ok(values)
        })?;

        reader.finish(values)
    }
}

impl<K, V> WireDecode for HashMap<K, V>
where
    K: Eq + Hash + WireDecode,
    V: WireDecode,
{
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let count = reader.read_value::<u32>()? as usize;
        let values = (0..count).try_fold(HashMap::with_capacity(count), |mut values, _| {
            let key = reader.read_value::<K>()?;
            let value = reader.read_value::<V>()?;
            if values.insert(key, value).is_some() {
                return Err(DecodeError::InvalidValue(InvalidWireValue::DuplicateMapKey));
            }
            Ok::<_, DecodeError>(values)
        })?;
        reader.finish(values)
    }
}

impl<K, V> WireDecode for BTreeMap<K, V>
where
    K: Ord + WireDecode,
    V: WireDecode,
{
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let mut reader = WireReader::new(buf);
        let count = reader.read_value::<u32>()? as usize;
        let values = (0..count).try_fold(BTreeMap::new(), |mut values, _| {
            let key = reader.read_value::<K>()?;
            let value = reader.read_value::<V>()?;
            if values.insert(key, value).is_some() {
                return Err(DecodeError::InvalidValue(InvalidWireValue::DuplicateMapKey));
            }
            Ok::<_, DecodeError>(values)
        })?;
        reader.finish(values)
    }
}

macro_rules! impl_wire_tuple_decode {
    ($($name:ident),+ $(,)?) => {
        impl<$($name: WireDecode),+> WireDecode for ($($name,)+) {
            fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
                let mut reader = WireReader::new(buf);
                let value = ($(
                    reader.read_value::<$name>()?,
                )+);
                reader.finish(value)
            }
        }
    };
}

impl_wire_tuple_decode!(A);
impl_wire_tuple_decode!(A, B);
impl_wire_tuple_decode!(A, B, C);
impl_wire_tuple_decode!(A, B, C, D);
impl_wire_tuple_decode!(A, B, C, D, E);
impl_wire_tuple_decode!(A, B, C, D, E, F);
impl_wire_tuple_decode!(A, B, C, D, E, F, G);
impl_wire_tuple_decode!(A, B, C, D, E, F, G, H);
impl_wire_tuple_decode!(A, B, C, D, E, F, G, H, I);
impl_wire_tuple_decode!(A, B, C, D, E, F, G, H, I, J);
impl_wire_tuple_decode!(A, B, C, D, E, F, G, H, I, J, K);
impl_wire_tuple_decode!(A, B, C, D, E, F, G, H, I, J, K, L);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::encode::WireEncode;

    #[test]
    fn decode_primitives() {
        let mut buf = [0u8; 8];

        42i32.encode_to(&mut buf);
        let (value, size) = i32::decode_from(&buf).unwrap();
        assert_eq!(value, 42);
        assert_eq!(size, 4);

        3.5f64.encode_to(&mut buf);
        let (value, size) = f64::decode_from(&buf).unwrap();
        assert!((value - 3.5).abs() < f64::EPSILON);
        assert_eq!(size, 8);

        true.encode_to(&mut buf);
        let (value, size) = bool::decode_from(&buf).unwrap();
        assert!(value);
        assert_eq!(size, 1);
    }

    #[test]
    fn decode_string() {
        let mut buf = [0u8; 32];
        let original = "hello".to_string();

        original.encode_to(&mut buf);
        let (decoded, size) = String::decode_from(&buf).unwrap();
        assert_eq!(decoded, "hello");
        assert_eq!(size, 9);
    }

    #[test]
    fn decode_option() {
        let mut buf = [0u8; 16];

        Some(42i32).encode_to(&mut buf);
        let (decoded, size) = Option::<i32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, Some(42));
        assert_eq!(size, 5);

        None::<i32>.encode_to(&mut buf);
        let (decoded, size) = Option::<i32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, None);
        assert_eq!(size, 1);
    }

    #[test]
    fn decode_vec_fixed() {
        let mut buf = [0u8; 32];
        let original = vec![1i32, 2, 3];

        original.encode_to(&mut buf);
        let (decoded, size) = Vec::<i32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, vec![1, 2, 3]);
        assert_eq!(size, 16);
    }

    #[test]
    fn decode_vec_variable() {
        let mut buf = [0u8; 64];
        let original = vec!["hi".to_string(), "there".to_string()];

        let written = original.encode_to(&mut buf);
        let (decoded, size) = Vec::<String>::decode_from(&buf).unwrap();
        assert_eq!(decoded, vec!["hi".to_string(), "there".to_string()]);
        assert_eq!(size, written);
    }

    #[test]
    fn decode_tuple() {
        let mut buf = [0u8; 64];
        let original = (7u32, "seven".to_string());

        let written = original.encode_to(&mut buf);
        let (decoded, size) = <(u32, String)>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(size, written);
    }

    #[test]
    fn decode_hash_map() {
        let mut buf = [0u8; 128];
        let original = HashMap::from([(String::from("one"), 1u32), (String::from("two"), 2u32)]);

        let written = original.encode_to(&mut buf);
        let (decoded, size) = HashMap::<String, u32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(size, written);
    }

    #[test]
    fn decode_btree_map() {
        let mut buf = [0u8; 128];
        let original = BTreeMap::from([(String::from("one"), 1u32), (String::from("two"), 2u32)]);

        let written = original.encode_to(&mut buf);
        let (decoded, size) = BTreeMap::<String, u32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(size, written);
    }

    #[test]
    fn decode_hash_map_rejects_duplicate_keys() {
        let mut buffer = [0u8; 128];
        let size = duplicate_key_map_buffer(&mut buffer);

        let error =
            HashMap::<String, u32>::decode_from(&buffer[..size]).expect_err("duplicate key");

        assert_eq!(
            error,
            DecodeError::InvalidValue(InvalidWireValue::DuplicateMapKey)
        );
    }

    #[test]
    fn decode_btree_map_rejects_duplicate_keys() {
        let mut buffer = [0u8; 128];
        let size = duplicate_key_map_buffer(&mut buffer);

        let error =
            BTreeMap::<String, u32>::decode_from(&buffer[..size]).expect_err("duplicate key");

        assert_eq!(
            error,
            DecodeError::InvalidValue(InvalidWireValue::DuplicateMapKey)
        );
    }

    #[test]
    fn roundtrip_complex() {
        let mut buf = [0u8; 128];

        let original: Vec<Option<String>> =
            vec![Some("hello".to_string()), None, Some("world".to_string())];

        let written = original.encode_to(&mut buf);
        let (decoded, size) = Vec::<Option<String>>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(size, written);
    }

    fn duplicate_key_map_buffer(buffer: &mut [u8]) -> usize {
        let mut offset = 0;
        buffer[..4].copy_from_slice(&2u32.to_le_bytes());
        offset += 4;
        offset += String::from("one").encode_to(&mut buffer[offset..]);
        offset += 1u32.encode_to(&mut buffer[offset..]);
        offset += String::from("one").encode_to(&mut buffer[offset..]);
        offset += 2u32.encode_to(&mut buffer[offset..]);
        offset
    }

    mod unit_type {
        use super::*;

        #[test]
        fn unit_decode_from_empty_buffer() {
            let buf: [u8; 0] = [];
            let (value, consumed) = <()>::decode_from(&buf).unwrap();
            assert_eq!(value, ());
            assert_eq!(consumed, 0);
        }

        #[test]
        fn unit_decode_from_nonempty_buffer() {
            let buf = [0xFF; 8];
            let (value, consumed) = <()>::decode_from(&buf).unwrap();
            assert_eq!(value, ());
            assert_eq!(consumed, 0);
        }

        #[test]
        fn unit_roundtrip() {
            let original = ();
            let mut buf = [0u8; 4];
            let written = original.encode_to(&mut buf);
            assert_eq!(written, 0);

            let (_decoded, consumed) = <()>::decode_from(&buf).unwrap();
            assert_eq!(consumed, 0);
        }

        #[test]
        fn result_ok_unit_roundtrip() {
            let original: Result<(), i32> = Ok(());
            let mut buf = [0u8; 16];
            let written = original.encode_to(&mut buf);

            let (decoded, consumed) = Result::<(), i32>::decode_from(&buf).unwrap();
            assert_eq!(decoded, Ok(()));
            assert_eq!(consumed, written);
        }

        #[test]
        fn result_err_with_unit_ok_roundtrip() {
            let original: Result<(), i32> = Err(99);
            let mut buf = [0u8; 16];
            let written = original.encode_to(&mut buf);

            let (decoded, consumed) = Result::<(), i32>::decode_from(&buf).unwrap();
            assert_eq!(decoded, Err(99));
            assert_eq!(consumed, written);
        }

        #[test]
        fn option_some_unit_roundtrip() {
            let original: Option<()> = Some(());
            let mut buf = [0u8; 4];
            let written = original.encode_to(&mut buf);

            let (decoded, consumed) = Option::<()>::decode_from(&buf).unwrap();
            assert_eq!(decoded, Some(()));
            assert_eq!(consumed, written);
        }
    }

    mod box_type {
        use super::*;

        #[test]
        fn box_i32_roundtrip() {
            let original = Box::new(42i32);
            let mut buf = [0u8; 4];
            original.encode_to(&mut buf);

            let (decoded, consumed) = Box::<i32>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
            assert_eq!(consumed, 4);
        }

        #[test]
        fn box_string_roundtrip() {
            let original = Box::new("hello".to_string());
            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, consumed) = Box::<String>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
            assert_eq!(consumed, 9);
        }

        #[test]
        fn box_decode_matches_bare_decode() {
            let mut buf = [0u8; 4];
            42i32.encode_to(&mut buf);

            let (bare, bare_consumed) = i32::decode_from(&buf).unwrap();
            let (boxed, box_consumed) = Box::<i32>::decode_from(&buf).unwrap();

            assert_eq!(*boxed, bare);
            assert_eq!(box_consumed, bare_consumed);
        }

        #[test]
        fn box_vec_roundtrip() {
            let original = Box::new(vec![1i32, 2, 3]);
            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, consumed) = Box::<Vec<i32>>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
            assert_eq!(consumed, 16);
        }

        #[test]
        fn nested_box_roundtrip() {
            let original = Box::new(Box::new(42i32));
            let mut buf = [0u8; 4];
            original.encode_to(&mut buf);

            let (decoded, consumed) = Box::<Box<i32>>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
            assert_eq!(consumed, 4);
        }

        #[test]
        fn option_box_roundtrip() {
            let original: Option<Box<i32>> = Some(Box::new(99));
            let mut buf = [0u8; 16];
            let written = original.encode_to(&mut buf);

            let (decoded, consumed) = Option::<Box<i32>>::decode_from(&buf).unwrap();
            assert_eq!(decoded, Some(Box::new(99)));
            assert_eq!(consumed, written);
        }
    }

    mod large_payload_roundtrip {
        use super::*;

        #[test]
        fn string_1mb_roundtrip() {
            let original: String = "x".repeat(1024 * 1024);

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn string_10mb_roundtrip() {
            let original: String = "y".repeat(10 * 1024 * 1024);

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn vec_100k_i32_roundtrip() {
            let original: Vec<i32> = (0..100_000).collect();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = Vec::<i32>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn vec_1m_i32_roundtrip() {
            let original: Vec<i32> = (0..1_000_000).collect();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = Vec::<i32>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn vec_10k_strings_roundtrip() {
            let original: Vec<String> = (0..10_000).map(|i| format!("item_{}", i)).collect();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = Vec::<String>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn nested_vecs_roundtrip() {
            let original: Vec<Vec<i32>> = (0..100).map(|_| (0..1000).collect()).collect();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = Vec::<Vec<i32>>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }
    }

    mod unicode_roundtrip {
        use super::*;

        #[test]
        fn emoji_roundtrip() {
            let original = "Hello 👋 World 🌍 🎉".to_string();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn cjk_roundtrip() {
            let original = "你好世界 こんにちは 안녕하세요".to_string();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn arabic_roundtrip() {
            let original = "مرحبا بالعالم".to_string();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn mixed_scripts_roundtrip() {
            let original = "Hello 你好 مرحبا Привет 🎉".to_string();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn combining_characters_roundtrip() {
            let original = "café naïve résumé".to_string();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn zero_width_joiner_emoji_roundtrip() {
            let original = "👨‍👩‍👧‍👦 👨‍💻 🏳️‍🌈".to_string();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn empty_string_roundtrip() {
            let original = String::new();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn null_byte_roundtrip() {
            let original = "hello\0world\0test".to_string();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn whitespace_variants_roundtrip() {
            let original = "tab\there\nnewline\rcarriage\u{00A0}nbsp".to_string();

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = String::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }

        #[test]
        fn vec_of_unicode_strings_roundtrip() {
            let original: Vec<String> = vec![
                "Hello".to_string(),
                "你好".to_string(),
                "مرحبا".to_string(),
                "👋🌍".to_string(),
            ];

            let mut buf = vec![0u8; original.wire_size()];
            original.encode_to(&mut buf);

            let (decoded, _) = Vec::<String>::decode_from(&buf).unwrap();
            assert_eq!(decoded, original);
        }
    }

    mod decode_errors {
        use super::*;

        #[test]
        fn string_buffer_too_small_for_length() {
            let buf = [0u8; 2];
            let result = String::decode_from(&buf);
            assert!(matches!(result, Err(DecodeError::BufferTooSmall)));
        }

        #[test]
        fn string_buffer_too_small_for_content() {
            let mut buf = [0u8; 8];
            buf[..4].copy_from_slice(&100u32.to_le_bytes());

            let result = String::decode_from(&buf);
            assert!(matches!(result, Err(DecodeError::BufferTooSmall)));
        }

        #[test]
        fn vec_buffer_too_small_for_count() {
            let buf = [0u8; 2];
            let result = Vec::<i32>::decode_from(&buf);
            assert!(matches!(result, Err(DecodeError::BufferTooSmall)));
        }

        #[test]
        fn vec_buffer_too_small_for_elements() {
            let mut buf = [0u8; 8];
            buf[..4].copy_from_slice(&100u32.to_le_bytes());

            let result = Vec::<i32>::decode_from(&buf);
            assert!(matches!(result, Err(DecodeError::BufferTooSmall)));
        }

        #[test]
        fn empty_buffer() {
            let buf: [u8; 0] = [];

            assert!(matches!(
                String::decode_from(&buf),
                Err(DecodeError::BufferTooSmall)
            ));
            assert!(matches!(
                Vec::<i32>::decode_from(&buf),
                Err(DecodeError::BufferTooSmall)
            ));
            assert!(matches!(
                i32::decode_from(&buf),
                Err(DecodeError::BufferTooSmall)
            ));
        }
    }
}
