use boltffi::__private::wire::{WireDecode, WireEncode};
use boltffi_tests::FixtureTree;

#[test]
fn leaf_roundtrip() {
    let original = FixtureTree::Leaf(42);
    let mut buf = vec![0u8; original.wire_size()];
    original.encode_to(&mut buf);

    let (decoded, consumed) = FixtureTree::decode_from(&buf).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(consumed, original.wire_size());
}

#[test]
fn shallow_node_roundtrip() {
    let original = FixtureTree::Node(
        Box::new(FixtureTree::Leaf(1)),
        Box::new(FixtureTree::Leaf(2)),
    );
    let mut buf = vec![0u8; original.wire_size()];
    original.encode_to(&mut buf);

    let (decoded, consumed) = FixtureTree::decode_from(&buf).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(consumed, original.wire_size());
}

#[test]
fn deep_balanced_tree_roundtrip() {
    fn build(depth: u32, start: i32) -> FixtureTree {
        if depth == 0 {
            FixtureTree::Leaf(start)
        } else {
            let left_size = 1 << (depth - 1);
            FixtureTree::Node(
                Box::new(build(depth - 1, start)),
                Box::new(build(depth - 1, start + left_size)),
            )
        }
    }

    let original = build(5, 0);
    let mut buf = vec![0u8; original.wire_size()];
    original.encode_to(&mut buf);

    let (decoded, consumed) = FixtureTree::decode_from(&buf).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(consumed, original.wire_size());
}

#[test]
fn asymmetric_tree_roundtrip() {
    let original = FixtureTree::Node(
        Box::new(FixtureTree::Leaf(-1)),
        Box::new(FixtureTree::Node(
            Box::new(FixtureTree::Leaf(2)),
            Box::new(FixtureTree::Node(
                Box::new(FixtureTree::Leaf(3)),
                Box::new(FixtureTree::Leaf(4)),
            )),
        )),
    );

    let mut buf = vec![0u8; original.wire_size()];
    original.encode_to(&mut buf);

    let (decoded, _) = FixtureTree::decode_from(&buf).unwrap();
    assert_eq!(decoded, original);
}
