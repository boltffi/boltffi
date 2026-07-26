use boltffi_core::wire::{DecodeError, InvalidWireValue, WireDecode, WireEncode};
use boltffi_macros::data;

extern crate self as boltffi;

pub mod __private {
    pub use boltffi_core::{
        EventSubscription, FfiBuf, FfiSpan, FfiStatus, Passable, RustFutureContinuationCallback,
        RustFutureHandle, StreamContinuationCallback, StreamPollResult, SubscriptionHandle,
        VecTransport, WaitResult, WirePassable, rustfuture, wire,
    };
}

mod primitives {
    use super::*;

    #[test]
    fn i8_boundary_values() {
        let cases = [i8::MIN, -1, 0, 1, i8::MAX];
        for &val in &cases {
            let mut buf = [0u8; 1];
            let written = val.encode_to(&mut buf);
            assert_eq!(written, 1);
            let (decoded, consumed) = i8::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
            assert_eq!(consumed, 1);
        }
    }

    #[test]
    fn i16_boundary_values() {
        let cases = [i16::MIN, -1, 0, 1, i16::MAX];
        for &val in &cases {
            let mut buf = [0u8; 2];
            val.encode_to(&mut buf);
            let (decoded, _) = i16::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn i32_boundary_values() {
        let cases = [i32::MIN, -1, 0, 1, i32::MAX];
        for &val in &cases {
            let mut buf = [0u8; 4];
            val.encode_to(&mut buf);
            let (decoded, _) = i32::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn i64_boundary_values() {
        let cases = [i64::MIN, -1, 0, 1, i64::MAX];
        for &val in &cases {
            let mut buf = [0u8; 8];
            val.encode_to(&mut buf);
            let (decoded, _) = i64::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn u8_boundary_values() {
        let cases = [u8::MIN, 1, 127, 128, u8::MAX];
        for &val in &cases {
            let mut buf = [0u8; 1];
            val.encode_to(&mut buf);
            let (decoded, _) = u8::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn u16_boundary_values() {
        let cases = [u16::MIN, 1, u16::MAX];
        for &val in &cases {
            let mut buf = [0u8; 2];
            val.encode_to(&mut buf);
            let (decoded, _) = u16::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn u32_boundary_values() {
        let cases = [u32::MIN, 1, u32::MAX];
        for &val in &cases {
            let mut buf = [0u8; 4];
            val.encode_to(&mut buf);
            let (decoded, _) = u32::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn u64_boundary_values() {
        let cases = [u64::MIN, 1, u64::MAX];
        for &val in &cases {
            let mut buf = [0u8; 8];
            val.encode_to(&mut buf);
            let (decoded, _) = u64::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn f32_special_values() {
        let cases = [0.0f32, -0.0, 1.0, -1.0, f32::MIN, f32::MAX, f32::EPSILON];
        for &val in &cases {
            let mut buf = [0u8; 4];
            val.encode_to(&mut buf);
            let (decoded, _) = f32::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn f32_nan_and_infinity() {
        let mut buf = [0u8; 4];

        f32::INFINITY.encode_to(&mut buf);
        let (decoded, _) = f32::decode_from(&buf).unwrap();
        assert!(decoded.is_infinite() && decoded.is_sign_positive());

        f32::NEG_INFINITY.encode_to(&mut buf);
        let (decoded, _) = f32::decode_from(&buf).unwrap();
        assert!(decoded.is_infinite() && decoded.is_sign_negative());

        f32::NAN.encode_to(&mut buf);
        let (decoded, _) = f32::decode_from(&buf).unwrap();
        assert!(decoded.is_nan());
    }

    #[test]
    fn f64_special_values() {
        let cases = [
            0.0f64,
            -0.0,
            1.0,
            -1.0,
            f64::MIN,
            f64::MAX,
            f64::EPSILON,
            std::f64::consts::PI,
        ];
        for &val in &cases {
            let mut buf = [0u8; 8];
            val.encode_to(&mut buf);
            let (decoded, _) = f64::decode_from(&buf).unwrap();
            assert_eq!(decoded, val);
        }
    }

    #[test]
    fn f64_nan_and_infinity() {
        let mut buf = [0u8; 8];

        f64::INFINITY.encode_to(&mut buf);
        let (decoded, _) = f64::decode_from(&buf).unwrap();
        assert!(decoded.is_infinite() && decoded.is_sign_positive());

        f64::NEG_INFINITY.encode_to(&mut buf);
        let (decoded, _) = f64::decode_from(&buf).unwrap();
        assert!(decoded.is_infinite() && decoded.is_sign_negative());

        f64::NAN.encode_to(&mut buf);
        let (decoded, _) = f64::decode_from(&buf).unwrap();
        assert!(decoded.is_nan());
    }

    #[test]
    fn bool_values() {
        let mut buf = [0u8; 1];

        true.encode_to(&mut buf);
        assert_eq!(buf[0], 1);
        let (decoded, _) = bool::decode_from(&buf).unwrap();
        assert!(decoded);

        false.encode_to(&mut buf);
        assert_eq!(buf[0], 0);
        let (decoded, _) = bool::decode_from(&buf).unwrap();
        assert!(!decoded);
    }

    #[test]
    fn bool_invalid_value_is_error() {
        let buf = [2u8];
        let result = bool::decode_from(&buf);
        assert!(matches!(
            result,
            Err(DecodeError::InvalidValue(InvalidWireValue::Bool))
        ));
    }

    #[test]
    fn primitive_buffer_too_small() {
        let buf = [0u8; 0];
        assert!(matches!(
            i32::decode_from(&buf),
            Err(DecodeError::BufferTooSmall)
        ));

        let buf = [0u8; 3];
        assert!(matches!(
            i32::decode_from(&buf),
            Err(DecodeError::BufferTooSmall)
        ));

        let buf = [0u8; 7];
        assert!(matches!(
            i64::decode_from(&buf),
            Err(DecodeError::BufferTooSmall)
        ));
    }
}

mod strings {
    use super::*;

    #[test]
    fn empty_string() {
        let original = String::new();
        assert_eq!(original.wire_size(), 4);

        let mut buf = [0u8; 4];
        let written = original.encode_to(&mut buf);
        assert_eq!(written, 4);
        assert_eq!(&buf, &[0, 0, 0, 0]);

        let (decoded, consumed) = String::decode_from(&buf).unwrap();
        assert_eq!(decoded, "");
        assert_eq!(consumed, 4);
    }

    #[test]
    fn ascii_string() {
        let original = "hello world".to_string();
        assert_eq!(original.wire_size(), 4 + 11);

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = String::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn unicode_string() {
        let original = "hello 世界 🌍".to_string();
        let expected_len = 4 + original.len();
        assert_eq!(original.wire_size(), expected_len);

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = String::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn string_with_null_bytes() {
        let original = "hello\0world".to_string();
        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = String::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.len(), 11);
    }

    #[test]
    fn long_string() {
        let original: String = "x".repeat(10000);
        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = String::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn string_buffer_too_small_for_length() {
        let buf = [0u8; 3];
        assert!(matches!(
            String::decode_from(&buf),
            Err(DecodeError::BufferTooSmall)
        ));
    }

    #[test]
    fn string_buffer_too_small_for_content() {
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&10u32.to_le_bytes());
        assert!(matches!(
            String::decode_from(&buf),
            Err(DecodeError::BufferTooSmall)
        ));
    }
}

mod options {
    use super::*;

    #[test]
    fn option_none_i32() {
        let original: Option<i32> = None;
        assert_eq!(original.wire_size(), 1);

        let mut buf = [0u8; 1];
        let written = original.encode_to(&mut buf);
        assert_eq!(written, 1);
        assert_eq!(buf[0], 0);

        let (decoded, consumed) = Option::<i32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, None);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn option_some_i32() {
        let original = Some(42i32);
        assert_eq!(original.wire_size(), 5);

        let mut buf = [0u8; 5];
        let written = original.encode_to(&mut buf);
        assert_eq!(written, 5);
        assert_eq!(buf[0], 1);

        let (decoded, consumed) = Option::<i32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, Some(42));
        assert_eq!(consumed, 5);
    }

    #[test]
    fn option_some_string() {
        let original = Some("hello".to_string());
        assert_eq!(original.wire_size(), 1 + 4 + 5);

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Option::<String>::decode_from(&buf).unwrap();
        assert_eq!(decoded, Some("hello".to_string()));
    }

    #[test]
    fn option_none_string() {
        let original: Option<String> = None;
        assert_eq!(original.wire_size(), 1);

        let mut buf = [0u8; 1];
        original.encode_to(&mut buf);

        let (decoded, _) = Option::<String>::decode_from(&buf).unwrap();
        assert_eq!(decoded, None);
    }

    #[test]
    fn nested_option_some_some() {
        let original: Option<Option<i32>> = Some(Some(42));
        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Option::<Option<i32>>::decode_from(&buf).unwrap();
        assert_eq!(decoded, Some(Some(42)));
    }

    #[test]
    fn nested_option_some_none() {
        let original: Option<Option<i32>> = Some(None);
        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Option::<Option<i32>>::decode_from(&buf).unwrap();
        assert_eq!(decoded, Some(None));
    }

    #[test]
    fn nested_option_none() {
        let original: Option<Option<i32>> = None;
        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Option::<Option<i32>>::decode_from(&buf).unwrap();
        assert_eq!(decoded, None);
    }
}

mod vecs {
    use super::*;

    #[test]
    fn empty_vec_i32() {
        let original: Vec<i32> = vec![];
        assert_eq!(original.wire_size(), 4);

        let mut buf = [0u8; 4];
        let written = original.encode_to(&mut buf);
        assert_eq!(written, 4);

        let (decoded, consumed) = Vec::<i32>::decode_from(&buf).unwrap();
        assert!(decoded.is_empty());
        assert_eq!(consumed, 4);
    }

    #[test]
    fn single_element_vec() {
        let original = vec![42i32];
        assert_eq!(original.wire_size(), 4 + 4);

        let mut buf = [0u8; 8];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<i32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, vec![42]);
    }

    #[test]
    fn vec_fixed_size_elements() {
        let original = vec![1i32, 2, 3, 4, 5];
        assert_eq!(original.wire_size(), 4 + 5 * 4);

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<i32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn vec_variable_size_elements() {
        let original = vec!["a".to_string(), "bb".to_string(), "ccc".to_string()];

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<String>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn empty_vec_string() {
        let original: Vec<String> = vec![];
        assert_eq!(original.wire_size(), 4);

        let mut buf = [0u8; 4];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<String>::decode_from(&buf).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn vec_with_empty_strings() {
        let original = vec!["".to_string(), "".to_string()];

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<String>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn nested_vec_fixed() {
        let original = vec![vec![1i32, 2], vec![3, 4, 5], vec![]];

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<Vec<i32>>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn nested_vec_variable() {
        let original = vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["c".to_string()],
            vec![],
        ];

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<Vec<String>>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn vec_of_options() {
        let original: Vec<Option<i32>> = vec![Some(1), None, Some(3), None];

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<Option<i32>>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn large_vec() {
        let original: Vec<i32> = (0..10000).collect();

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Vec::<i32>::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }
}

mod records {
    use super::*;

    #[test]
    fn fixed_size_record_roundtrip() {
        #[data]
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct Point {
            x: f64,
            y: f64,
        }

        let original = Point { x: 1.5, y: 2.5 };

        assert!(Point::is_fixed_size());
        assert_eq!(Point::fixed_size(), Some(16));
        assert_eq!(original.wire_size(), 16);

        let mut buf = vec![0u8; 16];
        let written = original.encode_to(&mut buf);
        assert_eq!(written, 16);

        let (decoded, consumed) = Point::decode_from(&buf).unwrap();
        assert_eq!(consumed, 16);
        assert_eq!(decoded, original);
    }

    #[test]
    fn fixed_size_record_boundary_values() {
        #[data]
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct Boundaries {
            min_i64: i64,
            max_i64: i64,
            min_f64: f64,
            max_f64: f64,
        }

        let original = Boundaries {
            min_i64: i64::MIN,
            max_i64: i64::MAX,
            min_f64: f64::MIN,
            max_f64: f64::MAX,
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Boundaries::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn variable_size_record_roundtrip() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct User {
            id: i32,
            name: String,
            score: f64,
        }

        let original = User {
            id: 42,
            name: "Alice".to_string(),
            score: 3.5,
        };

        assert!(!User::is_fixed_size());
        assert_eq!(User::fixed_size(), None);

        let expected_size = 4 + (4 + 5) + 8;
        assert_eq!(original.wire_size(), expected_size);

        let mut buf = vec![0u8; expected_size];
        let written = original.encode_to(&mut buf);
        assert_eq!(written, expected_size);

        let (decoded, consumed) = User::decode_from(&buf).unwrap();
        assert_eq!(consumed, expected_size);
        assert_eq!(decoded, original);
    }

    #[test]
    fn variable_record_with_empty_string() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Named {
            name: String,
        }

        let original = Named {
            name: String::new(),
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Named::decode_from(&buf).unwrap();
        assert_eq!(decoded.name, "");
    }

    #[test]
    fn variable_record_with_unicode() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Message {
            content: String,
        }

        let original = Message {
            content: "Hello 世界 🎉".to_string(),
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Message::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn nested_fixed_record_roundtrip() {
        #[data]
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct Inner {
            value: i32,
        }

        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Outer {
            inner: Inner,
            count: u64,
        }

        let original = Outer {
            inner: Inner { value: 100 },
            count: 999,
        };

        assert!(Outer::is_fixed_size());
        assert_eq!(Outer::fixed_size(), Some(12));

        let mut buf = vec![0u8; 12];
        let written = original.encode_to(&mut buf);
        assert_eq!(written, 12);

        let (decoded, _) = Outer::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn nested_variable_record_roundtrip() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Address {
            street: String,
            city: String,
        }

        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Person {
            name: String,
            address: Address,
        }

        let original = Person {
            name: "Bob".to_string(),
            address: Address {
                street: "123 Main St".to_string(),
                city: "Springfield".to_string(),
            },
        };

        assert!(!Person::is_fixed_size());

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Person::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn deeply_nested_fixed_record() {
        #[data]
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct Level3 {
            value: i32,
        }

        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Level2 {
            inner: Level3,
        }

        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Level1 {
            inner: Level2,
        }

        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Root {
            inner: Level1,
        }

        let original = Root {
            inner: Level1 {
                inner: Level2 {
                    inner: Level3 { value: 42 },
                },
            },
        };

        assert!(Root::is_fixed_size());

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Root::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_option_some() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct MaybeValue {
            id: i32,
            value: Option<i64>,
        }

        let original = MaybeValue {
            id: 1,
            value: Some(42),
        };

        assert!(!MaybeValue::is_fixed_size());

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = MaybeValue::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_option_none() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct MaybeValue {
            id: i32,
            value: Option<i64>,
        }

        let original = MaybeValue { id: 2, value: None };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = MaybeValue::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_option_string() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Labeled {
            label: Option<String>,
            value: i32,
        }

        let with_label = Labeled {
            label: Some("test".to_string()),
            value: 1,
        };
        let without_label = Labeled {
            label: None,
            value: 2,
        };

        let mut buf1 = vec![0u8; with_label.wire_size()];
        with_label.encode_to(&mut buf1);
        let (decoded1, _) = Labeled::decode_from(&buf1).unwrap();
        assert_eq!(decoded1, with_label);

        let mut buf2 = vec![0u8; without_label.wire_size()];
        without_label.encode_to(&mut buf2);
        let (decoded2, _) = Labeled::decode_from(&buf2).unwrap();
        assert_eq!(decoded2, without_label);
    }

    #[test]
    fn record_with_vec_fixed() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Scores {
            name: String,
            values: Vec<i32>,
        }

        let original = Scores {
            name: "player1".to_string(),
            values: vec![100, 200, 300],
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Scores::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_empty_vec() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Container {
            items: Vec<i32>,
        }

        let original = Container { items: vec![] };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Container::decode_from(&buf).unwrap();
        assert!(decoded.items.is_empty());
    }

    #[test]
    fn record_with_vec_variable() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Tags {
            tags: Vec<String>,
        }

        let original = Tags {
            tags: vec!["rust".to_string(), "ffi".to_string(), "wire".to_string()],
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Tags::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_nested_vec() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Matrix {
            rows: Vec<Vec<i32>>,
        }

        let original = Matrix {
            rows: vec![vec![1, 2, 3], vec![4, 5, 6]],
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Matrix::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_vec_of_options() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Sparse {
            values: Vec<Option<i32>>,
        }

        let original = Sparse {
            values: vec![Some(1), None, Some(3), None, Some(5)],
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Sparse::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_all_types() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Kitchen {
            flag: bool,
            byte: u8,
            short: i16,
            int: i32,
            long: i64,
            float: f32,
            double: f64,
            text: String,
            maybe: Option<i32>,
            list: Vec<i32>,
        }

        let original = Kitchen {
            flag: true,
            byte: 255,
            short: -1000,
            int: 42,
            long: i64::MAX,
            float: 3.5,
            double: 2.5,
            text: "kitchen sink".to_string(),
            maybe: Some(100),
            list: vec![1, 2, 3],
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Kitchen::decode_from(&buf).unwrap();
        assert_eq!(decoded.flag, original.flag);
        assert_eq!(decoded.byte, original.byte);
        assert_eq!(decoded.short, original.short);
        assert_eq!(decoded.int, original.int);
        assert_eq!(decoded.long, original.long);
        assert!((decoded.float - original.float).abs() < f32::EPSILON);
        assert!((decoded.double - original.double).abs() < f64::EPSILON);
        assert_eq!(decoded.text, original.text);
        assert_eq!(decoded.maybe, original.maybe);
        assert_eq!(decoded.list, original.list);
    }

    #[test]
    fn single_field_fixed_record() {
        #[data]
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct Single {
            value: i64,
        }

        let original = Single { value: 123456789 };
        assert!(Single::is_fixed_size());
        assert_eq!(Single::fixed_size(), Some(8));

        let mut buf = vec![0u8; 8];
        original.encode_to(&mut buf);

        let (decoded, _) = Single::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn single_field_variable_record() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Single {
            value: String,
        }

        let original = Single {
            value: "test".to_string(),
        };
        assert!(!Single::is_fixed_size());

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, _) = Single::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn many_fixed_fields() {
        #[data]
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct ManyFields {
            a: i32,
            b: i32,
            c: i32,
            d: i32,
            e: i32,
            f: i32,
            g: i32,
            h: i32,
            i: i32,
            j: i32,
        }

        let original = ManyFields {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: 5,
            f: 6,
            g: 7,
            h: 8,
            i: 9,
            j: 10,
        };

        assert!(ManyFields::is_fixed_size());
        assert_eq!(ManyFields::fixed_size(), Some(40));

        let mut buf = vec![0u8; 40];
        original.encode_to(&mut buf);

        let (decoded, _) = ManyFields::decode_from(&buf).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_size_field_roundtrip() {
        // A field named `size` used to collide with the internal `size`
        // binding in the generated wire decode code, producing
        // `let (size, size) = ...` — a duplicate binding error.
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Buffer {
            name: String,
            size: u32,
        }

        let original = Buffer {
            name: "payload.bin".to_string(),
            size: 4096,
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, consumed) = Buffer::decode_from(&buf).unwrap();
        assert_eq!(consumed, original.wire_size());
        assert_eq!(decoded, original);
    }

    #[test]
    fn record_with_size_and_position_fields_roundtrip() {
        // Fields named `size` and `position` both collided with internal
        // bindings in the generated wire decode code. `position` was
        // silently shadowed, corrupting subsequent field decodes.
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct Cursor {
            position: u32,
            size: u32,
            data: String,
        }

        let original = Cursor {
            position: 128,
            size: 512,
            data: "hello".to_string(),
        };

        let mut buf = vec![0u8; original.wire_size()];
        original.encode_to(&mut buf);

        let (decoded, consumed) = Cursor::decode_from(&buf).unwrap();
        assert_eq!(consumed, original.wire_size());
        assert_eq!(decoded, original);
    }
}

mod wire_size {
    use super::*;

    #[test]
    fn primitives_fixed_size() {
        assert!(i8::is_fixed_size());
        assert!(i16::is_fixed_size());
        assert!(i32::is_fixed_size());
        assert!(i64::is_fixed_size());
        assert!(u8::is_fixed_size());
        assert!(u16::is_fixed_size());
        assert!(u32::is_fixed_size());
        assert!(u64::is_fixed_size());
        assert!(f32::is_fixed_size());
        assert!(f64::is_fixed_size());
        assert!(bool::is_fixed_size());

        assert_eq!(i8::fixed_size(), Some(1));
        assert_eq!(i16::fixed_size(), Some(2));
        assert_eq!(i32::fixed_size(), Some(4));
        assert_eq!(i64::fixed_size(), Some(8));
        assert_eq!(f32::fixed_size(), Some(4));
        assert_eq!(f64::fixed_size(), Some(8));
        assert_eq!(bool::fixed_size(), Some(1));
    }

    #[test]
    fn string_not_fixed_size() {
        assert!(!String::is_fixed_size());
        assert_eq!(String::fixed_size(), None);
    }

    #[test]
    fn vec_not_fixed_size() {
        assert!(!<Vec<i32>>::is_fixed_size());
        assert_eq!(<Vec<i32>>::fixed_size(), None);
    }

    #[test]
    fn option_not_fixed_size() {
        assert!(!<Option<i32>>::is_fixed_size());
        assert_eq!(<Option<i32>>::fixed_size(), None);
    }

    #[test]
    fn wire_size_consistency() {
        let val = 42i32;
        let mut buf = vec![0u8; val.wire_size()];
        let written = val.encode_to(&mut buf);
        assert_eq!(written, val.wire_size());

        let s = "hello world".to_string();
        let mut buf = vec![0u8; s.wire_size()];
        let written = s.encode_to(&mut buf);
        assert_eq!(written, s.wire_size());

        let v = vec![1i32, 2, 3, 4, 5];
        let mut buf = vec![0u8; v.wire_size()];
        let written = v.encode_to(&mut buf);
        assert_eq!(written, v.wire_size());

        let opt = Some("test".to_string());
        let mut buf = vec![0u8; opt.wire_size()];
        let written = opt.encode_to(&mut buf);
        assert_eq!(written, opt.wire_size());
    }
}

mod enums {
    use super::*;

    #[test]
    fn unit_enum_roundtrip() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        assert!(Color::is_fixed_size());
        assert_eq!(Color::fixed_size(), Some(4));

        for (variant, expected_discriminant) in [
            (Color::Red, 0i32),
            (Color::Green, 1i32),
            (Color::Blue, 2i32),
        ] {
            assert_eq!(variant.wire_size(), 4);

            let mut buf = [0u8; 4];
            let written = variant.encode_to(&mut buf);
            assert_eq!(written, 4);
            assert_eq!(i32::from_le_bytes(buf), expected_discriminant);

            let (decoded, consumed) = Color::decode_from(&buf).unwrap();
            assert_eq!(consumed, 4);
            assert_eq!(decoded, variant);
        }
    }

    #[test]
    fn tuple_enum_roundtrip() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Value {
            Int(i32),
            Float(f64),
            Text(String),
        }

        assert!(!Value::is_fixed_size());

        let int_val = Value::Int(42);
        assert_eq!(int_val.wire_size(), 4 + 4);
        let mut buf = vec![0u8; int_val.wire_size()];
        int_val.encode_to(&mut buf);
        let (decoded, _) = Value::decode_from(&buf).unwrap();
        assert_eq!(decoded, int_val);

        let float_val = Value::Float(3.5);
        assert_eq!(float_val.wire_size(), 4 + 8);
        let mut buf = vec![0u8; float_val.wire_size()];
        float_val.encode_to(&mut buf);
        let (decoded, _) = Value::decode_from(&buf).unwrap();
        assert_eq!(decoded, float_val);

        let text_val = Value::Text("hello".to_string());
        assert_eq!(text_val.wire_size(), 4 + 4 + 5);
        let mut buf = vec![0u8; text_val.wire_size()];
        text_val.encode_to(&mut buf);
        let (decoded, _) = Value::decode_from(&buf).unwrap();
        assert_eq!(decoded, text_val);
    }

    #[test]
    fn struct_variant_enum_roundtrip() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Shape {
            Circle { radius: f64 },
            Rectangle { width: f64, height: f64 },
        }

        assert!(!Shape::is_fixed_size());

        let circle = Shape::Circle { radius: 5.0 };
        assert_eq!(circle.wire_size(), 4 + 8);
        let mut buf = vec![0u8; circle.wire_size()];
        circle.encode_to(&mut buf);
        let (decoded, _) = Shape::decode_from(&buf).unwrap();
        assert_eq!(decoded, circle);

        let rect = Shape::Rectangle {
            width: 10.0,
            height: 20.0,
        };
        assert_eq!(rect.wire_size(), 4 + 8 + 8);
        let mut buf = vec![0u8; rect.wire_size()];
        rect.encode_to(&mut buf);
        let (decoded, _) = Shape::decode_from(&buf).unwrap();
        assert_eq!(decoded, rect);
    }

    #[test]
    fn mixed_enum_roundtrip() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Event {
            None,
            Click(i32, i32),
            Message { text: String },
        }

        let none = Event::None;
        let mut buf = vec![0u8; none.wire_size()];
        none.encode_to(&mut buf);
        let (decoded, _) = Event::decode_from(&buf).unwrap();
        assert_eq!(decoded, none);

        let click = Event::Click(100, 200);
        let mut buf = vec![0u8; click.wire_size()];
        click.encode_to(&mut buf);
        let (decoded, _) = Event::decode_from(&buf).unwrap();
        assert_eq!(decoded, click);

        let msg = Event::Message {
            text: "hello".to_string(),
        };
        let mut buf = vec![0u8; msg.wire_size()];
        msg.encode_to(&mut buf);
        let (decoded, _) = Event::decode_from(&buf).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn enum_with_vec_field() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Container {
            Empty,
            Items(Vec<i32>),
        }

        let empty = Container::Empty;
        let mut buf = vec![0u8; empty.wire_size()];
        empty.encode_to(&mut buf);
        let (decoded, _) = Container::decode_from(&buf).unwrap();
        assert_eq!(decoded, empty);

        let items = Container::Items(vec![1, 2, 3, 4, 5]);
        let mut buf = vec![0u8; items.wire_size()];
        items.encode_to(&mut buf);
        let (decoded, _) = Container::decode_from(&buf).unwrap();
        assert_eq!(decoded, items);
    }

    #[test]
    fn enum_with_option_field() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum MaybeData {
            Nothing,
            Something(Option<i32>),
        }

        let nothing = MaybeData::Nothing;
        let mut buf = vec![0u8; nothing.wire_size()];
        nothing.encode_to(&mut buf);
        let (decoded, _) = MaybeData::decode_from(&buf).unwrap();
        assert_eq!(decoded, nothing);

        let some = MaybeData::Something(Some(42));
        let mut buf = vec![0u8; some.wire_size()];
        some.encode_to(&mut buf);
        let (decoded, _) = MaybeData::decode_from(&buf).unwrap();
        assert_eq!(decoded, some);

        let none = MaybeData::Something(None);
        let mut buf = vec![0u8; none.wire_size()];
        none.encode_to(&mut buf);
        let (decoded, _) = MaybeData::decode_from(&buf).unwrap();
        assert_eq!(decoded, none);
    }

    #[test]
    fn nested_enum_in_struct() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Status {
            Active,
            Inactive,
        }

        #[data]
        #[derive(Debug, Clone, PartialEq)]
        struct User {
            id: i32,
            status: Status,
        }

        let user = User {
            id: 1,
            status: Status::Active,
        };
        let mut buf = vec![0u8; user.wire_size()];
        user.encode_to(&mut buf);
        let (decoded, _) = User::decode_from(&buf).unwrap();
        assert_eq!(decoded, user);

        let user2 = User {
            id: 2,
            status: Status::Inactive,
        };
        let mut buf = vec![0u8; user2.wire_size()];
        user2.encode_to(&mut buf);
        let (decoded, _) = User::decode_from(&buf).unwrap();
        assert_eq!(decoded, user2);
    }

    #[test]
    fn enum_invalid_discriminant() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Binary {
            Zero,
            One,
        }

        let mut buf = [0u8; 4];
        buf.copy_from_slice(&99i32.to_le_bytes());

        let result = Binary::decode_from(&buf);
        assert!(matches!(
            result,
            Err(DecodeError::InvalidValue(InvalidWireValue::EnumTag))
        ));
    }

    #[test]
    fn enum_buffer_too_small() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Test {
            A,
        }

        let buf = [0u8; 3];
        let result = Test::decode_from(&buf);
        assert!(matches!(result, Err(DecodeError::BufferTooSmall)));
    }

    #[test]
    fn many_variants_enum() {
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Many {
            V0,
            V1,
            V2,
            V3,
            V4,
            V5,
            V6,
            V7,
            V8,
            V9,
        }

        assert!(Many::is_fixed_size());

        for (i, variant) in [
            Many::V0,
            Many::V1,
            Many::V2,
            Many::V3,
            Many::V4,
            Many::V5,
            Many::V6,
            Many::V7,
            Many::V8,
            Many::V9,
        ]
        .iter()
        .enumerate()
        {
            let mut buf = [0u8; 4];
            variant.encode_to(&mut buf);
            assert_eq!(i32::from_le_bytes(buf), i as i32);

            let (decoded, _) = Many::decode_from(&buf).unwrap();
            assert_eq!(&decoded, variant);
        }
    }

    #[test]
    fn enum_struct_variant_with_size_field_roundtrip() {
        // A variant field named `size` used to collide with the internal
        // `size` binding in the generated enum wire decode code.
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Packet {
            Empty,
            Data { size: u32, payload: String },
        }

        let data = Packet::Data {
            size: 256,
            payload: "abc".to_string(),
        };
        let mut buf = vec![0u8; data.wire_size()];
        data.encode_to(&mut buf);
        let (decoded, consumed) = Packet::decode_from(&buf).unwrap();
        assert_eq!(consumed, data.wire_size());
        assert_eq!(decoded, data);

        let empty = Packet::Empty;
        let mut buf = vec![0u8; empty.wire_size()];
        empty.encode_to(&mut buf);
        let (decoded, _) = Packet::decode_from(&buf).unwrap();
        assert_eq!(decoded, empty);
    }

    #[test]
    fn enum_tuple_variant_with_size_field_roundtrip() {
        // A tuple variant whose payload happens to be named `size` by
        // convention still works — tuple variants use `f0`/`f1` bindings,
        // but the internal `size`/`position` counters must still not
        // collide with any user-adjacent naming.
        #[data]
        #[derive(Debug, Clone, PartialEq)]
        enum Command {
            Resize(u32, u32),
            SetSize(u32),
        }

        let resize = Command::Resize(800, 600);
        let mut buf = vec![0u8; resize.wire_size()];
        resize.encode_to(&mut buf);
        let (decoded, _) = Command::decode_from(&buf).unwrap();
        assert_eq!(decoded, resize);

        let set = Command::SetSize(1024);
        let mut buf = vec![0u8; set.wire_size()];
        set.encode_to(&mut buf);
        let (decoded, _) = Command::decode_from(&buf).unwrap();
        assert_eq!(decoded, set);
    }
}
