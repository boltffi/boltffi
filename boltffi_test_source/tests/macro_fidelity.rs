use boltffi_ast::{PackageInfo, SourceContract};
use boltffi_test_source::{Error, scan_file};

fn scan(source: &str) -> Result<SourceContract, Error> {
    scan_file(
        syn::parse_file(source).expect("source parses"),
        PackageInfo::new("demo", None),
    )
}

#[test]
fn thiserror_helpers_do_not_export_unmarked_types() {
    let contract = scan(
        "
        #[derive(Debug, thiserror::Error)]
        #[error(\"request failed: {message}\")]
        pub struct ServiceError {
            pub message: String,
        }
        ",
    )
    .expect("an unmarked type scans");

    assert!(
        contract.records.is_empty(),
        "thiserror's `#[error(\"..\")]` is not a BoltFFI marker"
    );
}

#[test]
fn members_follow_the_configuration_with_every_cfg_off() {
    let contract = scan(
        "
        #[data]
        pub struct Point {
            pub x: f64,
            #[cfg(feature = \"depth\")]
            pub z: f64,
            #[cfg(not(feature = \"depth\"))]
            pub y: f64,
        }

        #[data]
        pub enum Mode {
            Fast,
            #[cfg(feature = \"slow\")]
            Slow,
        }

        #[export]
        pub struct Engine;

        #[export]
        impl Engine {
            pub fn new() -> Self {
                Self
            }

            #[cfg(boltffi_pending)]
            pub fn pending(&self) {}
        }

        #[export]
        pub trait Listener: Send + Sync {
            fn heard(&self);

            #[cfg(boltffi_pending)]
            fn pending(&self);
        }
        ",
    )
    .expect("conditional members scan");

    assert_eq!(
        contract.records[0].fields.len(),
        2,
        "the inactive field is dropped"
    );
    assert_eq!(
        contract.enums[0].variants.len(),
        1,
        "the inactive variant is dropped"
    );
    assert_eq!(
        contract.classes[0].methods.len(),
        1,
        "the inactive method is dropped"
    );
    assert_eq!(
        contract.traits[0].methods.len(),
        1,
        "the inactive trait method is dropped"
    );
}

#[test]
fn arguments_on_an_export_impl_are_refused() {
    let error = scan(
        "
        #[export]
        pub struct Engine;

        #[export(single_threaded)]
        impl Engine {
            pub fn new() -> Self {
                Self
            }
        }
        ",
    )
    .expect_err("`#[export] impl` takes no arguments");

    assert!(
        matches!(error, Error::Refused { .. }),
        "refused the way the macro refuses it, got {error}"
    );
}

#[test]
fn customs_on_one_generic_remote_resolve_by_their_arguments() {
    let contract = scan(
        "
        custom_type!(pub UtcTime, remote = chrono::DateTime<Utc>, repr = i64, into_ffi = a, try_from_ffi = b);
        custom_type!(pub LocalTime, remote = DateTime<Local>, repr = i64, into_ffi = c, try_from_ffi = d);

        #[export]
        pub fn utc() -> DateTime<chrono::Utc> {
            todo!()
        }

        #[export]
        pub fn local() -> DateTime<Local> {
            todo!()
        }
        ",
    )
    .expect("both customs scan");

    let returned = |name: &str| {
        let function = contract
            .functions
            .iter()
            .find(|function| function.name.spelling() == name)
            .expect("function is exported");
        format!("{:?}", function.returns)
    };
    assert!(
        returned("utc").contains("demo::UtcTime"),
        "{}",
        returned("utc")
    );
    assert!(
        returned("local").contains("demo::LocalTime"),
        "{}",
        returned("local")
    );
}

#[test]
fn a_remote_with_several_generic_arguments_is_declared() {
    let contract = scan(
        "
        custom_type!(pub Bag, remote = HashMap<String, u32>, repr = String, into_ffi = a, try_from_ffi = b);

        #[export]
        pub fn bag() -> Bag {
            todo!()
        }
        ",
    )
    .expect("the custom scans");

    assert_eq!(contract.customs.len(), 1, "`Bag` is declared");
}

#[test]
fn an_alias_with_two_targets_is_refused_where_it_is_used() {
    let source = |body: &str| {
        format!(
            "
            mod a {{
                type Id = u64;
                {body}
            }}

            mod b {{
                type Id = String;
            }}
            "
        )
    };

    scan(&source("")).expect("an unused ambiguous alias scans");
    let error = scan(&source("#[export] pub fn next() -> Id { 1 }"))
        .expect_err("`Id` could be either alias");

    assert!(
        matches!(&error, Error::Ambiguous { name } if name == "Id"),
        "got {error}"
    );
}

#[test]
fn type_markers_off_a_struct_or_enum_are_refused() {
    [
        "#[data] pub fn answer() -> u32 { 42 }",
        "#[error] pub fn answer() -> u32 { 42 }",
        "#[data] pub trait Listener { fn heard(&self); }",
        "#[data] pub const LIMIT: u32 = 42;",
    ]
    .into_iter()
    .for_each(|source| {
        let error = scan(source).expect_err("the marker declares a struct or enum");
        assert!(
            matches!(error, Error::Refused { .. }),
            "{source} got {error}"
        );
    });
}

#[test]
fn a_second_type_marker_is_refused() {
    [
        "#[data] #[export] pub struct Both { pub value: u32 }",
        "#[export] #[boltffi::error] pub enum Both { One }",
    ]
    .into_iter()
    .for_each(|source| {
        let error = scan(source).expect_err("a type is declared once");
        assert!(
            matches!(error, Error::Refused { .. }),
            "{source} got {error}"
        );
    });
}

#[test]
fn what_survives_configuration_drops_its_cfg() {
    let contract = scan(
        "
        #[cfg(not(feature = \"hidden\"))]
        #[data]
        pub struct Point {
            #[cfg(not(feature = \"depth\"))]
            pub y: f64,
        }

        #[data]
        pub enum Mode {
            #[cfg(not(feature = \"slow\"))]
            Fast,
        }

        #[export]
        pub struct Engine;

        #[export]
        impl Engine {
            #[cfg(not(boltffi_pending))]
            pub fn new() -> Self {
                Self
            }
        }

        #[export]
        pub trait Listener: Send + Sync {
            #[cfg(not(boltffi_pending))]
            fn heard(&self);
        }
        ",
    )
    .expect("conditional members scan");

    assert_eq!(contract.records[0].fields.len(), 1, "the field survives");
    assert_eq!(contract.classes[0].methods.len(), 1, "the method survives");
    assert!(
        !format!("{contract:?}").contains("cfg"),
        "no `cfg` reaches the contract"
    );
}

#[test]
fn types_the_macros_cannot_declare_are_refused() {
    [
        "#[data] pub enum Empty {}",
        "#[error] pub enum Empty {}",
        "#[data] pub union Bits { pub whole: u32, pub half: u16 }",
        "#[export] pub fn pick(#[cfg(feature = \"wide\")] a: u32, b: u32) -> u32 { b }",
    ]
    .into_iter()
    .for_each(|source| {
        let error = scan(source).expect_err("the macro refuses the item");
        assert!(
            matches!(error, Error::Refused { .. }),
            "{source} got {error}"
        );
    });
}

#[test]
fn boolean_cfg_predicates_are_evaluated() {
    let contract = scan(
        "
        #[cfg(true)]
        #[data]
        pub struct Kept { pub value: u32 }

        #[cfg(all(true, not(false)))]
        #[data]
        pub struct AlsoKept { pub value: u32 }

        #[cfg(false)]
        #[data]
        pub struct Dropped { pub value: u32 }

        #[cfg(any(false, feature = \"off\"))]
        #[data]
        pub struct AlsoDropped { pub value: u32 }
        ",
    )
    .expect("literal predicates scan");

    let mut names = contract
        .records
        .iter()
        .map(|record| record.name.spelling().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["AlsoKept", "Kept"]);
}

#[test]
fn an_alias_named_like_its_target_resolves_to_the_declaration() {
    let contract = scan(
        "
        mod geometry {
            #[data]
            pub struct Point { pub x: f64 }
        }

        pub type Point = geometry::Point;

        #[export]
        pub fn origin() -> Point {
            todo!()
        }
        ",
    )
    .expect("the alias resolves");

    let returned = format!("{:?}", contract.functions[0].returns);
    assert!(returned.contains("demo::Point"), "{returned}");
}

#[test]
fn a_declared_name_shared_with_an_alias_or_remote_is_refused() {
    [
        "
        mod a { pub type Id = u64; }
        mod b {
            #[data]
            pub struct Id { pub value: u32 }

            #[export]
            pub fn next() -> Id { todo!() }
        }
        ",
        "
        custom_type!(pub Stamp, remote = time::Point, repr = i64, into_ffi = a, try_from_ffi = b);

        #[data]
        pub struct Point { pub x: f64 }

        #[export]
        pub fn origin() -> Point { todo!() }
        ",
    ]
    .into_iter()
    .for_each(|source| {
        let error = scan(source).expect_err("the name could be either");
        assert!(
            matches!(error, Error::Ambiguous { .. }),
            "{source} got {error}"
        );
    });
}
