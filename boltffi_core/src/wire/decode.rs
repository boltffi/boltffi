use crate::wire::constants::*;
#[cfg(feature = "chrono")]
use chrono::{DateTime, Utc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[cfg(feature = "url")]
use url::Url;
#[cfg(feature = "uuid")]
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    BufferTooSmall,
    InvalidUtf8,
    InvalidBool,
    InvalidValue,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooSmall => write!(f, "BufferTooSmall"),
            Self::InvalidUtf8 => write!(f, "InvalidUtf8"),
            Self::InvalidBool => write!(f, "InvalidBool"),
            Self::InvalidValue => write!(f, "InvalidValue"),
        }
    }
}

pub type DecodeResult<T> = Result<(T, usize), DecodeError>;

pub trait WireDecode: Sized {
    const IS_BLITTABLE: bool = false;
    fn decode_from(buf: &[u8]) -> DecodeResult<Self>;
}

macro_rules! impl_wire_decode_primitive {
    ($($ty:ty),*) => {
        $(
            impl WireDecode for $ty {
                #[inline]
                fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
                    const SIZE: usize = core::mem::size_of::<$ty>();
                    let bytes: [u8; SIZE] = buf.get(..SIZE)
                        .ok_or(DecodeError::BufferTooSmall)?
                        .try_into()
                        .map_err(|_| DecodeError::BufferTooSmall)?;
                    Ok((<$ty>::from_le_bytes(bytes), SIZE))
                }
            }
        )*
    };
}

impl_wire_decode_primitive!(i8, i16, i32, i64, u8, u16, u32, u64, f32, f64);

impl WireDecode for bool {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        if buf.is_empty() {
            return Err(DecodeError::BufferTooSmall);
        }
        match buf[0] {
            0 => Ok((false, 1)),
            1 => Ok((true, 1)),
            _ => Err(DecodeError::InvalidBool),
        }
    }
}

impl WireDecode for isize {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let bytes: [u8; 8] = buf.get(..8).ok_or(DecodeError::BufferTooSmall)?.try_into().map_err(|_| DecodeError::BufferTooSmall)?;
        let value = i64::from_le_bytes(bytes) as isize;
        Ok((value, 8))
    }
}

impl WireDecode for usize {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let bytes: [u8; 8] = buf.get(..8).ok_or(DecodeError::BufferTooSmall)?.try_into().map_err(|_| DecodeError::BufferTooSmall)?;
        let value = u64::from_le_bytes(bytes) as usize;
        Ok((value, 8))
    }
}

impl WireDecode for String {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let len = u32::from_le_bytes(buf.get(..4).ok_or(DecodeError::BufferTooSmall)?.try_into().unwrap()) as usize;
        let total_size = 4 + len;
        let string_bytes = buf.get(4..total_size).ok_or(DecodeError::BufferTooSmall)?;
        let string = unsafe { core::str::from_utf8_unchecked(string_bytes) }.to_owned();
        Ok((string, total_size))
    }
}

impl WireDecode for Duration {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let (seconds, seconds_used) = u64::decode_from(buf)?;
        let (nanos, nanos_used) = u32::decode_from(buf.get(seconds_used..).ok_or(DecodeError::BufferTooSmall)?)?;
        if nanos >= 1_000_000_000 {
            return Err(DecodeError::InvalidValue);
        }
        Ok((Duration::new(seconds, nanos), seconds_used + nanos_used))
    }
}

impl WireDecode for SystemTime {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let (seconds, seconds_used) = i64::decode_from(buf)?;
        let (nanos, nanos_used) = u32::decode_from(buf.get(seconds_used..).ok_or(DecodeError::BufferTooSmall)?)?;
        if nanos >= 1_000_000_000 {
            return Err(DecodeError::InvalidValue);
        }

        let nanos_per_second = 1_000_000_000i128;
        let total_nanos = (seconds as i128) * nanos_per_second + (nanos as i128);

        let system_time = if total_nanos >= 0 {
            let duration = Duration::new((total_nanos / nanos_per_second) as u64, (total_nanos % nanos_per_second) as u32);
            UNIX_EPOCH + duration
        } else {
            let abs_total_nanos = (-total_nanos) as u128;
            let abs_seconds = (abs_total_nanos / (nanos_per_second as u128)) as u64;
            let abs_nanos = (abs_total_nanos % (nanos_per_second as u128)) as u32;
            UNIX_EPOCH - Duration::new(abs_seconds, abs_nanos)
        };

        Ok((system_time, seconds_used + nanos_used))
    }
}

#[cfg(feature = "uuid")]
impl WireDecode for Uuid {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let (hi, hi_used) = u64::decode_from(buf)?;
        let (lo, lo_used) = u64::decode_from(buf.get(hi_used..).ok_or(DecodeError::BufferTooSmall)?)?;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&hi.to_be_bytes());
        bytes[8..].copy_from_slice(&lo.to_be_bytes());
        Ok((Uuid::from_bytes(bytes), hi_used + lo_used))
    }
}

#[cfg(feature = "url")]
impl WireDecode for Url {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let (string, used) = String::decode_from(buf)?;
        let url = Url::parse(&string).map_err(|_| DecodeError::InvalidValue)?;
        Ok((url, used))
    }
}

#[cfg(feature = "chrono")]
impl WireDecode for DateTime<Utc> {
    #[inline]
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let (seconds, seconds_used) = i64::decode_from(buf)?;
        let (nanos, nanos_used) = u32::decode_from(buf.get(seconds_used..).ok_or(DecodeError::BufferTooSmall)?)?;
        let date_time = DateTime::from_timestamp(seconds, nanos).ok_or(DecodeError::InvalidValue)?;
        Ok((date_time, seconds_used + nanos_used))
    }
}

impl<T: WireDecode> WireDecode for Option<T> {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        if buf.is_empty() {
            return Err(DecodeError::BufferTooSmall);
        }

        match buf[0] {
            0 => Ok((None, OPTION_FLAG_SIZE)),
            1 => {
                let (value, value_size) = T::decode_from(&buf[OPTION_FLAG_SIZE..])?;
                Ok((Some(value), OPTION_FLAG_SIZE + value_size))
            }
            _ => Err(DecodeError::InvalidBool),
        }
    }
}

impl<T: WireDecode, E: WireDecode> WireDecode for Result<T, E> {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        if buf.is_empty() {
            return Err(DecodeError::BufferTooSmall);
        }

        match buf[0] {
            0 => {
                let (value, value_size) = T::decode_from(&buf[RESULT_TAG_SIZE..])?;
                Ok((Ok(value), RESULT_TAG_SIZE + value_size))
            }
            1 => {
                let (err, err_size) = E::decode_from(&buf[RESULT_TAG_SIZE..])?;
                Ok((Err(err), RESULT_TAG_SIZE + err_size))
            }
            _ => Err(DecodeError::InvalidBool),
        }
    }
}

pub trait FixedSizeWireDecode: Sized {
    const WIRE_SIZE: usize;
    fn decode_fixed(buf: &[u8]) -> Result<Self, DecodeError>;
}

macro_rules! impl_fixed_size_decode {
    ($($ty:ty),*) => {
        $(
            impl FixedSizeWireDecode for $ty {
                const WIRE_SIZE: usize = core::mem::size_of::<$ty>();

                #[inline]
                fn decode_fixed(buf: &[u8]) -> Result<Self, DecodeError> {
                    let bytes: [u8; Self::WIRE_SIZE] = buf.get(..Self::WIRE_SIZE)
                        .ok_or(DecodeError::BufferTooSmall)?
                        .try_into()
                        .map_err(|_| DecodeError::BufferTooSmall)?;
                    Ok(<$ty>::from_le_bytes(bytes))
                }
            }
        )*
    };
}

impl_fixed_size_decode!(i8, i16, i32, i64, u8, u16, u32, u64, f32, f64);

impl FixedSizeWireDecode for bool {
    const WIRE_SIZE: usize = 1;

    #[inline]
    fn decode_fixed(buf: &[u8]) -> Result<Self, DecodeError> {
        if buf.is_empty() {
            return Err(DecodeError::BufferTooSmall);
        }
        match buf[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::InvalidBool),
        }
    }
}

impl FixedSizeWireDecode for isize {
    const WIRE_SIZE: usize = 8;

    #[inline]
    fn decode_fixed(buf: &[u8]) -> Result<Self, DecodeError> {
        let bytes: [u8; 8] = buf.get(..8).ok_or(DecodeError::BufferTooSmall)?.try_into().map_err(|_| DecodeError::BufferTooSmall)?;
        Ok(i64::from_le_bytes(bytes) as isize)
    }
}

impl FixedSizeWireDecode for usize {
    const WIRE_SIZE: usize = 8;

    #[inline]
    fn decode_fixed(buf: &[u8]) -> Result<Self, DecodeError> {
        let bytes: [u8; 8] = buf.get(..8).ok_or(DecodeError::BufferTooSmall)?.try_into().map_err(|_| DecodeError::BufferTooSmall)?;
        Ok(u64::from_le_bytes(bytes) as usize)
    }
}

impl<T: WireDecode> WireDecode for Vec<T> {
    fn decode_from(buf: &[u8]) -> DecodeResult<Self> {
        let count_bytes: [u8; 4] = buf
            .get(..VEC_COUNT_SIZE)
            .ok_or(DecodeError::BufferTooSmall)?
            .try_into()
            .map_err(|_| DecodeError::BufferTooSmall)?;
        let count = u32::from_le_bytes(count_bytes) as usize;

        if count == 0 {
            return Ok((Vec::new(), VEC_COUNT_SIZE));
        }

        if T::IS_BLITTABLE {
            let element_size = core::mem::size_of::<T>();
            let data_size = count * element_size;
            let total_size = VEC_COUNT_SIZE + data_size;

            if buf.len() < total_size {
                return Err(DecodeError::BufferTooSmall);
            }

            let mut result = Vec::with_capacity(count);
            let src_ptr = buf[VEC_COUNT_SIZE..].as_ptr();
            unsafe {
                result.set_len(count);
                core::ptr::copy_nonoverlapping(src_ptr, result.as_mut_ptr() as *mut u8, data_size);
            }
            return Ok((result, total_size));
        }

        let mut result = Vec::with_capacity(count);
        let mut offset = VEC_COUNT_SIZE;

        for _ in 0..count {
            let (element, size) = T::decode_from(&buf[offset..])?;
            result.push(element);
            offset += size;
        }

        Ok((result, offset))
    }
}

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
    fn roundtrip_complex() {
        let mut buf = [0u8; 128];

        let original: Vec<Option<String>> = vec![Some("hello".to_string()), None, Some("world".to_string())];

        let written = original.encode_to(&mut buf);
        let (decoded, size) = Vec::<Option<String>>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(size, written);
    }

    mod large_payload_roundtrip {
        use super::*;
        use crate::wire::encode::WireSize;

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
        use crate::wire::encode::WireSize;

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
            let original: Vec<String> = vec!["Hello".to_string(), "你好".to_string(), "مرحبا".to_string(), "👋🌍".to_string()];

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

            assert!(matches!(String::decode_from(&buf), Err(DecodeError::BufferTooSmall)));
            assert!(matches!(Vec::<i32>::decode_from(&buf), Err(DecodeError::BufferTooSmall)));
            assert!(matches!(i32::decode_from(&buf), Err(DecodeError::BufferTooSmall)));
        }
    }
}
