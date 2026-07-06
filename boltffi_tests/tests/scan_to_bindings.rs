use boltffi_ast::PackageInfo;
use boltffi_binding::{
    Bindings, ClassDecl, ConstantDecl, ConstantValueDecl, Decl, DefaultValue, HandlePresence,
    HandleTarget, IncomingParam, IntegerValue, Native, ParamPlan, Primitive, Receive, RecordDecl,
    ReturnPlan, TypeRef, lower,
};
use boltffi_scan::scan_file;

const SOURCE: &str = "
    #[data]
    #[repr(C)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    #[data]
    pub enum Mode {
        VeryFast,
        Slow,
    }

    #[export]
    pub const DEFAULT_LIMIT: u32 = 42;

    #[export]
    pub const DEFAULT_MODE: Mode = Mode::VeryFast;

    custom_type!(
        pub UtcDateTime,
        remote = DateTime<Utc>,
        repr = i64,
        into_ffi = to_millis,
        try_from_ffi = from_millis,
    );

    #[data(impl)]
    impl Point {
        pub fn origin() -> Self {
            Self { x: 0.0, y: 0.0 }
        }

        pub fn distance(&self, other: Point) -> f64 {
            let dx = self.x - other.x;
            let dy = self.y - other.y;
            (dx * dx + dy * dy).sqrt()
        }
    }

    pub struct Engine;

    #[export(single_threaded)]
    impl Engine {
        pub fn new(seed: u64) -> Self {
            todo!()
        }

        pub fn start(&mut self) {}

        pub fn add_marker(&self) -> Marker {
            todo!()
        }
    }

    pub struct Marker;

    #[export(single_threaded)]
    impl Marker {
        pub fn id(&self) -> u64 {
            todo!()
        }
    }

    #[export]
    pub trait ValueCallback {
        fn on_value(&self, value: u32) -> u32;
    }

    #[export]
    pub fn invoke_callback(callback: impl ValueCallback, value: u32) -> u32 {
        callback.on_value(value)
    }

    #[export]
    pub fn make_handler() -> impl Fn(u32) -> u32 {
        |value| value
    }

    #[export]
    pub fn round_trip_time(value: DateTime<Utc>) -> DateTime<Utc> {
        value
    }
";

fn record_method_counts(record: &RecordDecl<Native>) -> (usize, usize) {
    (record.initializers().len(), record.methods().len())
}

fn class_method_counts(class: &ClassDecl<Native>) -> (usize, usize) {
    (class.initializers().len(), class.methods().len())
}

fn constant<'a>(bindings: &'a Bindings<Native>, name: &str) -> &'a ConstantDecl<Native> {
    bindings
        .decls()
        .iter()
        .find_map(|decl| match decl {
            Decl::Constant(constant) if constant.name().as_path_string() == name => {
                Some(constant.as_ref())
            }
            _ => None,
        })
        .expect("constant declaration")
}

fn class<'a>(bindings: &'a Bindings<Native>, name: &str) -> &'a ClassDecl<Native> {
    bindings
        .decls()
        .iter()
        .find_map(|decl| match decl {
            Decl::Class(class) if class.name().as_path_string() == name => Some(class.as_ref()),
            _ => None,
        })
        .expect("class declaration")
}

#[test]
fn scans_and_lowers_point_contract_to_bindings() {
    let file = syn::parse_str(SOURCE).expect("parse source fixture");
    let contract = scan_file(file, PackageInfo::new("demo", None)).expect("scan");
    let bindings = lower::<Native>(&contract).expect("lower");

    let records = bindings
        .decls()
        .iter()
        .filter(|decl| matches!(decl, Decl::Record(_)))
        .count();
    let functions = bindings
        .decls()
        .iter()
        .filter(|decl| matches!(decl, Decl::Function(_)))
        .count();
    let callbacks = bindings
        .decls()
        .iter()
        .filter(|decl| matches!(decl, Decl::Callback(_)))
        .count();
    let classes = bindings
        .decls()
        .iter()
        .filter(|decl| matches!(decl, Decl::Class(_)))
        .count();
    let constants = bindings
        .decls()
        .iter()
        .filter(|decl| matches!(decl, Decl::Constant(_)))
        .count();
    let customs = bindings
        .decls()
        .iter()
        .filter(|decl| matches!(decl, Decl::CustomType(_)))
        .count();
    assert_eq!(records, 1, "Point lowers to one record");
    assert_eq!(functions, 3, "functions lower from scanned exports");
    assert_eq!(callbacks, 1, "ValueCallback lowers to one callback");
    assert_eq!(classes, 2, "Engine and Marker lower to classes");
    assert_eq!(constants, 2, "exported constants lower to constants");
    assert_eq!(customs, 1, "custom types lower from scanned macros");

    let record = bindings
        .decls()
        .iter()
        .find_map(|decl| match decl {
            Decl::Record(record) => Some(record.as_ref()),
            _ => None,
        })
        .expect("record declaration");

    assert_eq!(record_method_counts(record), (1, 1));

    let engine = class(&bindings, "engine");
    let marker = class(&bindings, "marker");

    assert_eq!(class_method_counts(engine), (1, 2));
    assert_eq!(engine.initializers()[0].name().as_path_string(), "new");
    assert_eq!(engine.methods()[0].name().as_path_string(), "start");
    assert_eq!(
        engine.methods()[0].callable().receiver(),
        Some(Receive::ByMutRef)
    );
    assert_eq!(engine.methods()[1].name().as_path_string(), "add::marker");
    assert_eq!(
        engine.methods()[1].callable().receiver(),
        Some(Receive::ByRef)
    );
    match engine.methods()[1].callable().returns().plan() {
        ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Class(class_id),
            presence: HandlePresence::Required,
            ..
        } => assert_eq!(class_id, &marker.id()),
        other => panic!("expected required marker handle return, got {other:?}"),
    }

    match constant(&bindings, "default::limit").value() {
        ConstantValueDecl::Inline { ty, value, .. } => {
            assert_eq!(ty, &TypeRef::Primitive(Primitive::U32));
            assert_eq!(value, &DefaultValue::Integer(IntegerValue::new(42)));
        }
        other => panic!("expected inline integer constant, got {other:?}"),
    }

    match constant(&bindings, "default::mode").value() {
        ConstantValueDecl::Inline {
            value:
                DefaultValue::EnumVariant {
                    enum_name,
                    variant_name,
                },
            ..
        } => {
            assert_eq!(enum_name.as_path_string(), "mode");
            assert_eq!(variant_name.as_path_string(), "very::fast");
        }
        other => panic!("expected inline enum constant, got {other:?}"),
    }

    let custom = bindings
        .decls()
        .iter()
        .find_map(|decl| match decl {
            Decl::CustomType(custom) => Some(custom.as_ref()),
            _ => None,
        })
        .expect("custom type declaration");
    let custom_id = custom.id();
    assert_eq!(custom.representation(), &TypeRef::Primitive(Primitive::I64));

    let round_trip = bindings
        .decls()
        .iter()
        .find_map(|decl| match decl {
            Decl::Function(function) if function.name().as_path_string() == "round::trip::time" => {
                Some(function.as_ref())
            }
            _ => None,
        })
        .expect("round_trip_time function");
    match round_trip.callable().params()[0].payload() {
        IncomingParam::Value(ParamPlan::Encoded { ty, .. }) => {
            assert_eq!(ty, &TypeRef::Custom(custom_id));
        }
        other => panic!("expected encoded custom param, got {other:?}"),
    }
    match round_trip.callable().returns().plan() {
        ReturnPlan::EncodedViaReturnSlot { ty, .. } => {
            assert_eq!(ty, &TypeRef::Custom(custom_id));
        }
        other => panic!("expected encoded custom return, got {other:?}"),
    }
}
