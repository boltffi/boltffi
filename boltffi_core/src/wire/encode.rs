use crate::wire::constants::*;
#[cfg(feature = "chrono")]
use chrono::{DateTime, Utc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[cfg(feature = "url")]
use url::Url;
#[cfg(feature = "uuid")]
use uuid::Uuid;

pub trait WireSize {
    fn is_fixed_size() -> bool
    where
        Self: Sized,
    {
        false
    }
    fn fixed_size() -> Option<usize>
    where
        Self: Sized,
    {
        None
    }
    fn wire_size(&self) -> usize;
}

pub trait WireEncode: WireSize {
    const IS_BLITTABLE: bool = false;
    fn encode_to(&self, buf: &mut [u8]) -> usize;
}

macro_rules! impl_wire_primitive {
    ($($ty:ty),*) => {
        $(
            impl WireSize for $ty {
                #[inline]
                fn is_fixed_size() -> bool { true }

                #[inline]
                fn fixed_size() -> Option<usize> { Some(core::mem::size_of::<$ty>()) }

                #[inline]
                fn wire_size(&self) -> usize { core::mem::size_of::<$ty>() }
            }

            impl WireEncode for $ty {
                const IS_BLITTABLE: bool = true;

                #[inline]
                fn encode_to(&self, buf: &mut [u8]) -> usize {
                    let bytes = self.to_le_bytes();
                    buf[..bytes.len()].copy_from_slice(&bytes);
                    bytes.len()
                }
            }
        )*
    };
}

impl_wire_primitive!(i8, i16, i32, i64, u8, u16, u32, u64, f32, f64);

impl WireSize for bool {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(1)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        1
    }
}

impl WireEncode for bool {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        buf[0] = if *self { 1 } else { 0 };
        1
    }
}

impl WireSize for isize {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(8)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        8
    }
}

impl WireEncode for isize {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let value = *self as i64;
        let bytes = value.to_le_bytes();
        buf[..8].copy_from_slice(&bytes);
        8
    }
}

impl WireSize for usize {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(8)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        8
    }
}

impl WireEncode for usize {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let value = *self as u64;
        let bytes = value.to_le_bytes();
        buf[..8].copy_from_slice(&bytes);
        8
    }
}

impl WireSize for str {
    #[inline]
    fn wire_size(&self) -> usize {
        STRING_LEN_SIZE + self.len()
    }
}

impl WireEncode for str {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let len = self.len() as u32;
        buf[..STRING_LEN_SIZE].copy_from_slice(&len.to_le_bytes());
        buf[STRING_LEN_SIZE..STRING_LEN_SIZE + self.len()].copy_from_slice(self.as_bytes());
        STRING_LEN_SIZE + self.len()
    }
}

impl WireSize for String {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        self.as_str().wire_size()
    }
}

impl WireEncode for String {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        self.as_str().encode_to(buf)
    }
}

impl WireSize for Duration {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(12)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        12
    }
}

impl WireEncode for Duration {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let seconds = self.as_secs();
        let nanos = self.subsec_nanos();
        seconds.encode_to(&mut buf[..8]);
        nanos.encode_to(&mut buf[8..12]);
        12
    }
}

impl WireSize for SystemTime {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(12)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        12
    }
}

impl WireEncode for SystemTime {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let nanos_per_second = 1_000_000_000i128;
        let total_nanos: i128 = match self.duration_since(UNIX_EPOCH) {
            Ok(duration) => (duration.as_secs() as i128) * nanos_per_second + (duration.subsec_nanos() as i128),
            Err(error) => {
                let duration = error.duration();
                -((duration.as_secs() as i128) * nanos_per_second + (duration.subsec_nanos() as i128))
            }
        };

        let seconds = total_nanos.div_euclid(nanos_per_second) as i64;
        let nanos = total_nanos.rem_euclid(nanos_per_second) as u32;

        seconds.encode_to(&mut buf[..8]);
        nanos.encode_to(&mut buf[8..12]);
        12
    }
}

#[cfg(feature = "uuid")]
impl WireSize for Uuid {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(16)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        16
    }
}

#[cfg(feature = "uuid")]
impl WireEncode for Uuid {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let bytes = self.as_bytes();
        let hi = u64::from_be_bytes(bytes[..8].try_into().expect("uuid hi bytes"));
        let lo = u64::from_be_bytes(bytes[8..].try_into().expect("uuid lo bytes"));
        hi.encode_to(&mut buf[..8]);
        lo.encode_to(&mut buf[8..16]);
        16
    }
}

#[cfg(feature = "url")]
impl WireSize for Url {
    #[inline]
    fn wire_size(&self) -> usize {
        self.as_str().wire_size()
    }
}

#[cfg(feature = "url")]
impl WireEncode for Url {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        self.as_str().encode_to(buf)
    }
}

#[cfg(feature = "chrono")]
impl WireSize for DateTime<Utc> {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(12)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        12
    }
}

#[cfg(feature = "chrono")]
impl WireEncode for DateTime<Utc> {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let seconds = self.timestamp();
        let nanos = self.timestamp_subsec_nanos();
        seconds.encode_to(&mut buf[..8]);
        nanos.encode_to(&mut buf[8..12]);
        12
    }
}

impl<T: WireSize> WireSize for Option<T> {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        match self {
            Some(value) => OPTION_FLAG_SIZE + value.wire_size(),
            None => OPTION_FLAG_SIZE,
        }
    }
}

impl<T: WireEncode> WireEncode for Option<T> {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        match self {
            Some(value) => {
                buf[0] = 1;
                OPTION_FLAG_SIZE + value.encode_to(&mut buf[OPTION_FLAG_SIZE..])
            }
            None => {
                buf[0] = 0;
                OPTION_FLAG_SIZE
            }
        }
    }
}

impl<T: WireEncode> WireSize for Vec<T> {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        if T::IS_BLITTABLE {
            VEC_COUNT_SIZE + self.len() * core::mem::size_of::<T>()
        } else {
            VEC_COUNT_SIZE + self.iter().map(|element| element.wire_size()).sum::<usize>()
        }
    }
}

impl<T: WireEncode> WireEncode for Vec<T> {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let count = self.len() as u32;
        buf[..VEC_COUNT_SIZE].copy_from_slice(&count.to_le_bytes());

        if self.is_empty() {
            return VEC_COUNT_SIZE;
        }

        if T::IS_BLITTABLE {
            let byte_count = self.len() * core::mem::size_of::<T>();
            unsafe {
                core::ptr::copy_nonoverlapping(self.as_ptr() as *const u8, buf.as_mut_ptr().add(VEC_COUNT_SIZE), byte_count);
            }
            VEC_COUNT_SIZE + byte_count
        } else {
            let mut offset = VEC_COUNT_SIZE;
            self.iter().for_each(|element| {
                offset += element.encode_to(&mut buf[offset..]);
            });
            offset
        }
    }
}

impl<T: WireEncode> WireSize for [T] {
    #[inline]
    fn wire_size(&self) -> usize {
        if T::IS_BLITTABLE {
            VEC_COUNT_SIZE + core::mem::size_of_val(self)
        } else {
            VEC_COUNT_SIZE + self.iter().map(|element| element.wire_size()).sum::<usize>()
        }
    }
}

impl<T: WireEncode> WireEncode for [T] {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let count = self.len() as u32;
        buf[..VEC_COUNT_SIZE].copy_from_slice(&count.to_le_bytes());

        if self.is_empty() {
            return VEC_COUNT_SIZE;
        }

        if T::IS_BLITTABLE {
            let byte_count = core::mem::size_of_val(self);
            unsafe {
                core::ptr::copy_nonoverlapping(self.as_ptr() as *const u8, buf.as_mut_ptr().add(VEC_COUNT_SIZE), byte_count);
            }
            VEC_COUNT_SIZE + byte_count
        } else {
            let mut offset = VEC_COUNT_SIZE;
            self.iter().for_each(|element| {
                offset += element.encode_to(&mut buf[offset..]);
            });
            offset
        }
    }
}

impl<T: WireSize, E: WireSize> WireSize for Result<T, E> {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        match self {
            Ok(value) => RESULT_TAG_SIZE + value.wire_size(),
            Err(err) => RESULT_TAG_SIZE + err.wire_size(),
        }
    }
}

impl<T: WireEncode, E: WireEncode> WireEncode for Result<T, E> {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        match self {
            Ok(value) => {
                buf[0] = 0;
                RESULT_TAG_SIZE + value.encode_to(&mut buf[RESULT_TAG_SIZE..])
            }
            Err(err) => {
                buf[0] = 1;
                RESULT_TAG_SIZE + err.encode_to(&mut buf[RESULT_TAG_SIZE..])
            }
        }
    }
}

impl WireSize for () {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(0)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        0
    }
}

impl WireEncode for () {
    #[inline]
    fn encode_to(&self, _buf: &mut [u8]) -> usize {
        0
    }
}

impl<T: WireSize + ?Sized> WireSize for &T {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        (*self).wire_size()
    }
}

impl<T: WireEncode + ?Sized> WireEncode for &T {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        (*self).encode_to(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_primitives() {
        let mut buf = [0u8; 8];

        let written = 42i32.encode_to(&mut buf);
        assert_eq!(written, 4);
        assert_eq!(&buf[..4], &[42, 0, 0, 0]);

        let written = 3.5f64.encode_to(&mut buf);
        assert_eq!(written, 8);

        let written = true.encode_to(&mut buf);
        assert_eq!(written, 1);
        assert_eq!(buf[0], 1);

        let written = false.encode_to(&mut buf);
        assert_eq!(written, 1);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn encode_string() {
        let mut buf = [0u8; 32];
        let s = "hello";

        let written = s.encode_to(&mut buf);
        assert_eq!(written, 9); // 4 (len) + 5 (bytes)
        assert_eq!(&buf[..4], &[5, 0, 0, 0]); // len = 5
        assert_eq!(&buf[4..9], b"hello");
    }

    #[test]
    fn encode_option_some() {
        let mut buf = [0u8; 16];
        let opt: Option<i32> = Some(42);

        let written = opt.encode_to(&mut buf);
        assert_eq!(written, 5); // 1 (flag) + 4 (i32)
        assert_eq!(buf[0], 1); // is_some
        assert_eq!(&buf[1..5], &[42, 0, 0, 0]);
    }

    #[test]
    fn encode_option_none() {
        let mut buf = [0u8; 16];
        let opt: Option<i32> = None;

        let written = opt.encode_to(&mut buf);
        assert_eq!(written, 1);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn encode_vec_fixed_size() {
        let mut buf = [0u8; 32];
        let vec: Vec<i32> = vec![1, 2, 3];

        let written = vec.encode_to(&mut buf);
        assert_eq!(written, 16); // 4 (count) + 3 * 4 (elements)
        assert_eq!(&buf[..4], &[3, 0, 0, 0]); // count = 3
        assert_eq!(&buf[4..8], &[1, 0, 0, 0]);
        assert_eq!(&buf[8..12], &[2, 0, 0, 0]);
        assert_eq!(&buf[12..16], &[3, 0, 0, 0]);
    }

    #[test]
    fn encode_vec_variable_size() {
        let mut buf = [0u8; 64];
        let vec: Vec<String> = vec!["hi".to_string(), "there".to_string()];

        let written = vec.encode_to(&mut buf);
        assert_eq!(written, 4 + 6 + 9);
        assert_eq!(&buf[..4], &[2, 0, 0, 0]);
    }

    #[test]
    fn wire_size_calculations() {
        assert_eq!(42i32.wire_size(), 4);
        assert_eq!("hello".wire_size(), 9);
        assert_eq!(Some(42i32).wire_size(), 5);
        assert_eq!(None::<i32>.wire_size(), 1);

        let vec: Vec<i32> = vec![1, 2, 3];
        assert_eq!(vec.wire_size(), 16);

        let vec: Vec<String> = vec!["hi".to_string(), "there".to_string()];
        assert_eq!(vec.wire_size(), 4 + 6 + 9);
    }

    mod large_payloads {
        use super::*;

        #[test]
        fn large_string_1mb() {
            let size = 1024 * 1024;
            let large_string: String = "x".repeat(size);

            assert_eq!(large_string.wire_size(), 4 + size);

            let mut buf = vec![0u8; large_string.wire_size()];
            let written = large_string.encode_to(&mut buf);

            assert_eq!(written, 4 + size);
            assert_eq!(&buf[4..], large_string.as_bytes());
        }

        #[test]
        fn large_string_10mb() {
            let size = 10 * 1024 * 1024;
            let large_string: String = "y".repeat(size);

            assert_eq!(large_string.wire_size(), 4 + size);

            let mut buf = vec![0u8; large_string.wire_size()];
            let written = large_string.encode_to(&mut buf);

            assert_eq!(written, 4 + size);
        }

        #[test]
        fn large_vec_100k_elements() {
            let count = 100_000;
            let large_vec: Vec<i32> = (0..count).collect();

            assert_eq!(large_vec.wire_size(), 4 + count as usize * 4);

            let mut buf = vec![0u8; large_vec.wire_size()];
            let written = large_vec.encode_to(&mut buf);

            assert_eq!(written, 4 + count as usize * 4);

            let stored_count = u32::from_le_bytes(buf[..4].try_into().unwrap());
            assert_eq!(stored_count, count as u32);
        }

        #[test]
        fn large_vec_1m_elements() {
            let count = 1_000_000;
            let large_vec: Vec<i32> = (0..count).collect();

            let mut buf = vec![0u8; large_vec.wire_size()];
            let written = large_vec.encode_to(&mut buf);

            assert_eq!(written, 4 + count as usize * 4);
        }

        #[test]
        fn large_vec_of_strings() {
            let count = 10_000;
            let large_vec: Vec<String> = (0..count).map(|i| format!("item_{}", i)).collect();

            let expected_size: usize = 4 + large_vec.iter().map(|s| 4 + s.len()).sum::<usize>();
            assert_eq!(large_vec.wire_size(), expected_size);

            let mut buf = vec![0u8; large_vec.wire_size()];
            let written = large_vec.encode_to(&mut buf);

            assert_eq!(written, expected_size);
        }

        #[test]
        fn nested_large_structures() {
            let inner_count: usize = 1000;
            let outer_count: usize = 100;

            let nested: Vec<Vec<i32>> = (0..outer_count).map(|_| (0..inner_count as i32).collect()).collect();

            let inner_size = 4 + inner_count * 4;
            let expected_size = 4 + outer_count * inner_size;
            assert_eq!(nested.wire_size(), expected_size);

            let mut buf = vec![0u8; nested.wire_size()];
            let written = nested.encode_to(&mut buf);

            assert_eq!(written, expected_size);
        }
    }

    mod unicode {
        use super::*;

        #[test]
        fn ascii_string() {
            let s = "Hello, World!";
            assert_eq!(s.wire_size(), 4 + 13);
        }

        #[test]
        fn emoji_string() {
            let s = "Hello 👋 World 🌍";
            assert_eq!(s.wire_size(), 4 + s.len());

            let mut buf = vec![0u8; s.wire_size()];
            s.encode_to(&mut buf);

            assert_eq!(&buf[4..], s.as_bytes());
        }

        #[test]
        fn cjk_characters() {
            let s = "你好世界";
            assert_eq!(s.len(), 12);
            assert_eq!(s.wire_size(), 4 + 12);

            let mut buf = vec![0u8; s.wire_size()];
            s.encode_to(&mut buf);

            assert_eq!(&buf[4..], s.as_bytes());
        }

        #[test]
        fn arabic_rtl_text() {
            let s = "مرحبا بالعالم";
            assert_eq!(s.wire_size(), 4 + s.len());

            let mut buf = vec![0u8; s.wire_size()];
            s.encode_to(&mut buf);

            assert_eq!(&buf[4..], s.as_bytes());
        }

        #[test]
        fn mixed_scripts() {
            let s = "Hello 你好 مرحبا 🎉";
            assert_eq!(s.wire_size(), 4 + s.len());

            let mut buf = vec![0u8; s.wire_size()];
            s.encode_to(&mut buf);

            assert_eq!(&buf[4..], s.as_bytes());
        }

        #[test]
        fn combining_characters() {
            let s = "é";
            assert_eq!(s.chars().count(), 1);
            assert_eq!(s.len(), 2);
            assert_eq!(s.wire_size(), 4 + 2);

            let combining = "e\u{0301}";
            assert_eq!(combining.chars().count(), 2);
            assert_eq!(combining.len(), 3);
            assert_eq!(combining.wire_size(), 4 + 3);
        }

        #[test]
        fn zero_width_joiner_emoji() {
            let family = "👨‍👩‍👧‍👦";
            assert_eq!(family.wire_size(), 4 + family.len());

            let mut buf = vec![0u8; family.wire_size()];
            family.encode_to(&mut buf);

            assert_eq!(&buf[4..], family.as_bytes());
        }

        #[test]
        fn empty_string() {
            let s = "";
            assert_eq!(s.wire_size(), 4);

            let mut buf = vec![0u8; 4];
            let written = s.encode_to(&mut buf);

            assert_eq!(written, 4);
            assert_eq!(&buf, &[0, 0, 0, 0]);
        }

        #[test]
        fn single_byte_boundary() {
            let s = "\u{7F}";
            assert_eq!(s.len(), 1);
            assert_eq!(s.wire_size(), 4 + 1);
        }

        #[test]
        fn two_byte_boundary() {
            let s = "\u{80}";
            assert_eq!(s.len(), 2);
            assert_eq!(s.wire_size(), 4 + 2);

            let s = "\u{7FF}";
            assert_eq!(s.len(), 2);
        }

        #[test]
        fn three_byte_boundary() {
            let s = "\u{800}";
            assert_eq!(s.len(), 3);

            let s = "\u{FFFF}";
            assert_eq!(s.len(), 3);
        }

        #[test]
        fn four_byte_boundary() {
            let s = "\u{10000}";
            assert_eq!(s.len(), 4);

            let s = "\u{10FFFF}";
            assert_eq!(s.len(), 4);
        }

        #[test]
        fn string_with_newlines_and_tabs() {
            let s = "line1\nline2\tcolumn";
            assert_eq!(s.wire_size(), 4 + s.len());

            let mut buf = vec![0u8; s.wire_size()];
            s.encode_to(&mut buf);

            assert_eq!(&buf[4..], s.as_bytes());
        }

        #[test]
        fn string_with_null_bytes() {
            let s = "hello\0world";
            assert_eq!(s.len(), 11);
            assert_eq!(s.wire_size(), 4 + 11);

            let mut buf = vec![0u8; s.wire_size()];
            s.encode_to(&mut buf);

            assert_eq!(&buf[4..], s.as_bytes());
        }
    }

    #[allow(clippy::assertions_on_constants)]
    mod blittable {
        use super::*;

        #[test]
        fn primitive_is_blittable() {
            assert!(i32::IS_BLITTABLE);
            assert!(f64::IS_BLITTABLE);
            assert!(u8::IS_BLITTABLE);
        }

        #[test]
        fn string_is_not_blittable() {
            assert!(!String::IS_BLITTABLE);
        }

        #[test]
        fn vec_i32_encoding_matches_raw_memory() {
            let vec: Vec<i32> = vec![1, 2, 3, 0x7FFFFFFF, -1];
            let mut buf = vec![0u8; vec.wire_size()];
            vec.encode_to(&mut buf);

            assert_eq!(&buf[0..4], &5u32.to_le_bytes());

            let expected_bytes: Vec<u8> = vec.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(&buf[4..], &expected_bytes);
        }

        #[test]
        fn vec_f64_encoding_matches_raw_memory() {
            let vec: Vec<f64> = vec![1.5, -2.25, std::f64::consts::PI];
            let mut buf = vec![0u8; vec.wire_size()];
            vec.encode_to(&mut buf);

            assert_eq!(&buf[0..4], &3u32.to_le_bytes());

            let expected_bytes: Vec<u8> = vec.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(&buf[4..], &expected_bytes);
        }

        #[test]
        fn empty_blittable_vec() {
            let vec: Vec<i32> = vec![];
            assert_eq!(vec.wire_size(), 4);

            let mut buf = vec![0u8; 4];
            let written = vec.encode_to(&mut buf);
            assert_eq!(written, 4);
            assert_eq!(&buf, &[0, 0, 0, 0]);
        }

        #[test]
        fn blittable_wire_size_is_exact() {
            let vec: Vec<i32> = vec![1, 2, 3];
            assert_eq!(vec.wire_size(), 4 + 3 * 4);

            let vec: Vec<f64> = vec![1.0, 2.0];
            assert_eq!(vec.wire_size(), 4 + 2 * 8);

            let vec: Vec<u8> = vec![1, 2, 3, 4, 5];
            assert_eq!(vec.wire_size(), 4 + 5);
        }
    }
}
