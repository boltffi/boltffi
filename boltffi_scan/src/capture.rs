//! Scans one macro invocation's item without whole-crate knowledge.
//!
//! Ids come out as `$self::Name` placeholders and every named type reference becomes a
//! `$slot:N` leaf paired with the written path in [`CapturedItem::slots`], for the macro
//! to project through the compiler at the invocation site.

use boltffi_ast::{
    ClassDef, ConstantDef, ConstantOwner, CustomTypeDef, EnumDef, FunctionDef, MethodDef,
    RecordDef, RecordId, StreamDef, TraitDef, TypeExpr,
};

use crate::declared_types::DeclaredTypes;
use crate::path::{ModulePath, ModuleScope};
use crate::{ScanError, items};

/// One deferred reference, in the form the record emitter projects it through.
pub enum SlotSource {
    /// A type whose descriptor comes from its `TypeDesc` impl.
    Type(Box<syn::Type>),
    /// A trait bound whose descriptor is the value-namespace const the trait's
    /// definition site emits under the trait's own name.
    TraitValue(syn::Path),
}

impl quote::ToTokens for SlotSource {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self {
            Self::Type(ty) => ty.to_tokens(tokens),
            Self::TraitValue(path) => path.to_tokens(tokens),
        }
    }
}

/// One item scanned in deferred-resolution mode.
pub struct CapturedItem<Def> {
    /// The declaration with placeholder ids and slot leaves.
    pub def: Def,
    /// Written references, in slot-index order.
    pub slots: Vec<SlotSource>,
}

/// Captures a `#[data]` struct as a record definition.
pub fn capture_struct(item: &syn::ItemStruct) -> Result<CapturedItem<RecordDef>, ScanError> {
    captured(|scope, declared_types| items::record::scan_item(item, scope, declared_types))
}

/// Captures a `#[data]` enum as an enum definition.
pub fn capture_enum(item: &syn::ItemEnum) -> Result<CapturedItem<EnumDef>, ScanError> {
    captured(|scope, declared_types| items::enumeration::scan_item(item, scope, declared_types))
}

/// Captures an `#[error]` struct as a record definition carrying the error marker.
pub fn capture_error_struct(item: &syn::ItemStruct) -> Result<CapturedItem<RecordDef>, ScanError> {
    let mut captured = capture_struct(item)?;
    crate::marker::Marker::Error.append_value_attrs(&mut captured.def.user_attrs);
    Ok(captured)
}

/// Captures an `#[error]` enum as an enum definition carrying the error marker.
pub fn capture_error_enum(item: &syn::ItemEnum) -> Result<CapturedItem<EnumDef>, ScanError> {
    let mut captured = capture_enum(item)?;
    crate::marker::Marker::Error.append_value_attrs(&mut captured.def.user_attrs);
    Ok(captured)
}

/// Captures an `#[export]` free function as a function definition.
pub fn capture_function(item: &syn::ItemFn) -> Result<CapturedItem<FunctionDef>, ScanError> {
    captured(|scope, declared_types| items::function::scan_item(item, scope, declared_types))
}

/// Captures an `#[export] impl` block as a class definition.
///
/// The `marker_args` parameter is the export marker's argument tokens, which carry the
/// class thread-safety choice.
pub fn capture_class(
    item: &syn::ItemImpl,
    marker_args: proc_macro2::TokenStream,
) -> Result<CapturedItem<ClassDef>, ScanError> {
    let thread_safety = crate::marker::export_marker_from_args(marker_args)?.class_thread_safety();
    captured(|scope, declared_types| {
        items::class::scan_item(item, scope, declared_types, thread_safety)
    })
}

/// Captures the associated constants of an `#[export] impl` block as constant definitions.
pub fn capture_class_constants(
    item: &syn::ItemImpl,
) -> Result<CapturedItem<Vec<ConstantDef>>, ScanError> {
    captured(|scope, declared_types| {
        let owner = ConstantOwner::Class(items::class::resolve_id(item, scope, declared_types)?);
        items::constant::scan_associated_in_impl(item, &owner, scope, declared_types)
    })
}

/// Captures the `#[ffi_stream]` methods of an `#[export] impl` block as stream definitions.
pub fn capture_streams(item: &syn::ItemImpl) -> Result<CapturedItem<Vec<StreamDef>>, ScanError> {
    captured(|scope, declared_types| items::stream::scan_item(item, scope, declared_types))
}

/// Captures an `#[export]` constant as a constant definition.
pub fn capture_constant(item: &syn::ItemConst) -> Result<CapturedItem<ConstantDef>, ScanError> {
    captured(|scope, declared_types| items::constant::scan_item(item, scope, declared_types))
}

/// Captures an exported callback trait as a trait definition.
pub fn capture_trait(item: &syn::ItemTrait) -> Result<CapturedItem<TraitDef>, ScanError> {
    captured(|scope, declared_types| items::callback::scan_item(item, scope, declared_types))
}

/// Captures a `custom_type!` invocation's spec tokens as a custom type definition.
pub fn capture_custom(
    tokens: proc_macro2::TokenStream,
) -> Result<CapturedItem<CustomTypeDef>, ScanError> {
    captured(|scope, declared_types| {
        items::custom_type::scan_macro_tokens(tokens.clone(), scope, declared_types)
    })
}

/// Captures a `#[custom_ffi]` trait impl as a custom type definition.
pub fn capture_custom_ffi(item: &syn::ItemImpl) -> Result<CapturedItem<CustomTypeDef>, ScanError> {
    captured(|scope, declared_types| {
        items::custom_type::scan_trait_impl_item(item, scope, declared_types)
    })
}

/// One `interned_string_pool!` declaration, captured from its invocation tokens.
pub struct CapturedPool {
    /// The pool type's declared name.
    pub name: String,
    /// Static values addressable by wire id, in declaration order.
    pub values: Vec<String>,
}

/// Captures an `interned_string_pool!` invocation's declaration tokens.
pub fn capture_interned_string_pool(
    tokens: proc_macro2::TokenStream,
) -> Result<CapturedPool, ScanError> {
    let spec = items::interned_string_pool::Spec::parse_tokens(tokens)?;
    Ok(CapturedPool {
        name: spec.name().to_string(),
        values: spec.values().to_vec(),
    })
}

/// Methods captured from one `#[data(impl)]` block, targeting a slot-deferred type.
pub struct CapturedMethods {
    /// The impl target as a slot-deferred type expression.
    pub target: TypeExpr,
    /// The impl target's written spelling, for stable dedup keys.
    pub spelling: String,
    /// The block's methods, with ids nested under the target's slot placeholder.
    pub methods: Vec<MethodDef>,
    /// The block's associated constants, with ids nested under the target's slot
    /// placeholder and a provisional owner replaced when the target resolves.
    pub constants: Vec<ConstantDef>,
    /// Written references, in slot-index order.
    pub slots: Vec<SlotSource>,
}

/// Captures a `#[data(impl)]` methods block against its slot-deferred target.
pub fn capture_methods(item: &syn::ItemImpl) -> Result<CapturedMethods, ScanError> {
    let scope = ModuleScope::with_spans(ModulePath::root("$self"), &[], None);
    let mut declared_types = DeclaredTypes::deferred();
    let scanner = crate::type_expr::Scanner::new(&declared_types, &scope);
    let target = scanner.scan(&item.self_ty)?;
    let parent = match &target {
        TypeExpr::Record { id, .. } => id.as_str().to_owned(),
        _ => return Err(ScanError::unsupported_type(&item.self_ty)),
    };
    let methods = items::impl_methods::scan_value_methods(item, &parent, &scope, &declared_types)?;
    let owner = ConstantOwner::Record(RecordId::new(parent));
    let constants =
        items::constant::scan_associated_in_impl(item, &owner, &scope, &declared_types)?;
    let spelling = crate::spelling::path(&match &*item.self_ty {
        syn::Type::Path(path) => path.path.clone(),
        _ => return Err(ScanError::unsupported_type(&item.self_ty)),
    });
    Ok(CapturedMethods {
        target,
        spelling,
        methods,
        constants,
        slots: declared_types.take_slots(),
    })
}

fn captured<Def>(
    scan: impl FnOnce(&ModuleScope, &DeclaredTypes) -> Result<Def, ScanError>,
) -> Result<CapturedItem<Def>, ScanError> {
    let scope = ModuleScope::with_spans(ModulePath::root("$self"), &[], None);
    let mut declared_types = DeclaredTypes::deferred();
    let def = scan(&scope, &declared_types)?;
    Ok(CapturedItem {
        def,
        slots: declared_types.take_slots(),
    })
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{ReturnDef, TypeExpr};
    use quote::ToTokens;

    use super::*;

    #[test]
    fn captures_a_struct_with_deferred_references() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Route {
                pub start: Point,
                pub stops: Vec<geometry::Point>,
                pub label: String,
            }
        };

        let captured = capture_struct(&item).expect("struct captures");

        assert_eq!(captured.def.id.as_str(), "$self::Route");
        assert_eq!(
            captured.slots.len(),
            2,
            "each distinct written path gets one slot"
        );
        assert_eq!(
            captured.slots[0].to_token_stream().to_string(),
            "Point",
            "slots keep the written spelling"
        );
        assert_eq!(
            captured.slots[1].to_token_stream().to_string(),
            "geometry :: Point"
        );
        assert!(
            matches!(
                &captured.def.fields[0].type_expr,
                TypeExpr::Record { id, .. } if id.as_str() == "$slot:0"
            ),
            "named references defer to their slot"
        );
        assert!(
            matches!(
                &captured.def.fields[1].type_expr,
                TypeExpr::Vec(inner)
                    if matches!(inner.as_ref(), TypeExpr::Record { id, .. } if id.as_str() == "$slot:1")
            ),
            "container spellings stay structural around the slot"
        );
        assert!(
            matches!(&captured.def.fields[2].type_expr, TypeExpr::String),
            "standard leaves never take a slot"
        );
    }

    #[test]
    fn captures_trait_bounds_as_value_namespace_slots() {
        let item: syn::ItemFn = syn::parse_quote! {
            pub fn apply(callback: impl ValueCallback, boxed: Box<dyn ValueCallback + Send>) -> i32 {
                0
            }
        };

        let captured = capture_function(&item).expect("function captures");

        assert_eq!(
            captured.slots.len(),
            1,
            "impl and dyn references to one trait share a slot"
        );
        assert!(
            matches!(&captured.slots[0], SlotSource::TraitValue(path)
                if path.to_token_stream().to_string() == "ValueCallback"),
            "trait slots reference the descriptor const by the written path"
        );
        assert!(
            matches!(
                &captured.def.parameters[0].type_expr,
                TypeExpr::ImplTrait(bounds)
                    if matches!(
                        &bounds.base,
                        boltffi_ast::BaseTrait::Named { id, .. } if id.as_str() == "$slot:0"
                    )
            ),
            "the impl-Trait base defers to the trait slot"
        );
    }

    #[test]
    fn captures_generic_remote_paths_as_whole_type_slots() {
        let item: syn::ItemFn = syn::parse_quote! {
            pub fn width(span: Range<u32>) -> u32 {
                0
            }
        };

        let captured = capture_function(&item).expect("function captures");

        assert_eq!(captured.slots.len(), 1);
        assert!(
            matches!(&captured.slots[0], SlotSource::Type(ty)
                if ty.to_token_stream().to_string() == "Range < u32 >"),
            "the generic path defers whole, compiler-resolved at the custom's impl"
        );
        assert!(
            matches!(
                &captured.def.parameters[0].type_expr,
                TypeExpr::Record { id, path }
                    if id.as_str() == "$slot:0" && path.last().is_some_and(|segment| segment.name.as_str() == "Range")
            ),
            "the provisional leaf carries the argument-less written path"
        );
    }

    #[test]
    fn captures_repeated_references_into_one_slot() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Segment {
                pub from: Point,
                pub to: Point,
            }
        };

        let captured = capture_struct(&item).expect("struct captures");

        assert_eq!(captured.slots.len(), 1, "identical spellings share a slot");
        assert!(
            matches!(
                &captured.def.fields[1].type_expr,
                TypeExpr::Record { id, .. } if id.as_str() == "$slot:0"
            ),
            "the second reference reuses the first slot"
        );
    }

    #[test]
    fn captures_an_enum_with_payload_references() {
        let item: syn::ItemEnum = syn::parse_quote! {
            pub enum Shape {
                Empty,
                At(Point),
            }
        };

        let captured = capture_enum(&item).expect("enum captures");

        assert_eq!(captured.def.id.as_str(), "$self::Shape");
        assert_eq!(captured.slots.len(), 1);
    }

    #[test]
    fn captures_error_items_with_the_error_marker() {
        let item: syn::ItemEnum = syn::parse_quote! {
            pub enum MathError {
                DivisionByZero,
            }
        };

        let captured = capture_error_enum(&item).expect("error enum captures");

        assert!(
            captured.def.user_attrs.iter().any(|attr| {
                attr.path
                    .last()
                    .is_some_and(|segment| segment.name.as_str() == "error")
            }),
            "the error marker rides as a user attr"
        );
    }

    #[test]
    fn captures_a_function_signature() {
        let item: syn::ItemFn = syn::parse_quote! {
            pub fn nearest(route: Route, count: u32) -> Option<Point> {
                unimplemented!()
            }
        };

        let captured = capture_function(&item).expect("function captures");

        assert_eq!(captured.def.id.as_str(), "$self::nearest");
        assert_eq!(captured.slots.len(), 2, "Route and Point defer");
        assert!(
            matches!(
                &captured.def.returns,
                ReturnDef::Value(TypeExpr::Option(inner))
                    if matches!(inner.as_ref(), TypeExpr::Record { id, .. } if id.as_str() == "$slot:1")
            ),
            "the return type defers through the option"
        );
    }

    #[test]
    fn captures_a_class_impl_with_methods() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Counter {
                pub fn new(initial: i32) -> Self {
                    unimplemented!()
                }

                pub fn add(&self, amount: i32) -> i32 {
                    unimplemented!()
                }
            }
        };

        let captured =
            capture_class(&item, proc_macro2::TokenStream::new()).expect("class impl captures");

        assert_eq!(captured.def.id.as_str(), "$self::Counter");
        assert_eq!(captured.def.methods.len(), 2);
        assert_eq!(
            captured.def.methods[0].id.as_str(),
            "$self::Counter::new",
            "method ids nest under the class placeholder"
        );
        assert_eq!(
            captured.def.thread_safety,
            boltffi_ast::ClassThreadSafety::default()
        );
    }

    #[test]
    fn captures_single_threaded_class_thread_safety() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Counter {
                pub fn bump(&mut self) {}
            }
        };

        let captured =
            capture_class(&item, quote::quote!(single_threaded)).expect("class impl captures");

        assert_eq!(
            captured.def.thread_safety,
            boltffi_ast::ClassThreadSafety::UnsafeSingleThreaded,
            "the export marker's thread-safety choice rides the fragment"
        );
    }

    #[test]
    fn captures_stream_methods_with_deferred_item_types() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Engine {
                pub fn start(&self) {}

                #[ffi_stream(item = Point, mode = "batch")]
                pub fn points(&self) -> Arc<EventSubscription<Point>> {
                    unimplemented!()
                }
            }
        };

        let captured = capture_streams(&item).expect("streams capture");

        assert_eq!(captured.def.len(), 1, "only stream methods produce streams");
        assert_eq!(captured.def[0].id.as_str(), "$self::Engine::points");
        assert_eq!(
            captured.def[0].owner.as_ref().map(|owner| owner.as_str()),
            Some("$self::Engine")
        );
        assert_eq!(captured.def[0].mode, boltffi_ast::StreamMode::Batch);
        assert!(
            matches!(
                &captured.def[0].item_type,
                TypeExpr::Record { id, .. } if id.as_str() == "$slot:0"
            ),
            "the item type defers to its slot"
        );
        assert_eq!(
            captured.slots.len(),
            1,
            "the runtime wrapper types never take slots"
        );
    }

    #[test]
    fn captures_stream_methods_with_qualified_runtime_types() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Engine {
                #[ffi_stream(item = i32)]
                pub fn values(&self) -> std::sync::Arc<boltffi::EventSubscription<i32>> {
                    unimplemented!()
                }
            }
        };

        let captured = capture_streams(&item).expect("streams capture");

        assert_eq!(captured.def.len(), 1);
        assert!(matches!(&captured.def[0].item_type, TypeExpr::Primitive(_)));
    }

    #[test]
    fn rejects_stream_methods_without_subscription_returns() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Engine {
                #[ffi_stream(item = i32)]
                pub fn values(&self) -> i32 {
                    unimplemented!()
                }
            }
        };

        assert!(
            capture_streams(&item).is_err(),
            "the subscription return shape stays required"
        );
    }

    #[test]
    fn captures_a_callback_trait() {
        let item: syn::ItemTrait = syn::parse_quote! {
            pub trait Listener {
                fn on_event(&self, message: String);
            }
        };

        let captured = capture_trait(&item).expect("trait captures");

        assert_eq!(captured.def.id.as_str(), "$self::Listener");
        assert_eq!(captured.def.methods.len(), 1);
    }

    #[test]
    fn captures_associated_constants_under_the_target_placeholder() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Point {
                pub const ORIGIN: Point = Point { x: 0.0 };
                pub const UNIT: Self = Point { x: 1.0 };
                const PRIVATE: f64 = 0.5;
                #[skip]
                pub const HIDDEN: f64 = 0.25;

                pub fn doubled(&self) -> Point {
                    unimplemented!()
                }
            }
        };

        let captured = capture_methods(&item).expect("methods block captures");

        assert_eq!(
            captured.constants.len(),
            2,
            "private and skipped constants stay out"
        );
        assert_eq!(captured.constants[0].id.as_str(), "$slot:0::ORIGIN");
        assert!(
            matches!(
                &captured.constants[0].type_expr,
                TypeExpr::Record { id, .. } if id.as_str() == "$slot:0"
            ),
            "the declared type defers to the target's slot"
        );
        assert!(
            matches!(&captured.constants[1].type_expr, TypeExpr::SelfType),
            "Self stays a self type"
        );
    }

    #[test]
    fn captures_class_constants_with_the_class_owner() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Counter {
                pub const MAX: i32 = 9;

                pub fn new() -> Self {
                    unimplemented!()
                }
            }
        };

        let captured = capture_class_constants(&item).expect("class constants capture");

        assert_eq!(captured.def.len(), 1);
        assert_eq!(captured.def[0].id.as_str(), "$self::Counter::MAX");
        assert!(
            matches!(
                &captured.def[0].owner,
                Some(ConstantOwner::Class(id)) if id.as_str() == "$self::Counter"
            ),
            "the owner carries the class placeholder for minting"
        );
    }

    #[test]
    fn captures_a_constant() {
        let item: syn::ItemConst = syn::parse_quote! {
            pub const LIMIT: u32 = 42;
        };

        let captured = capture_constant(&item).expect("constant captures");

        assert_eq!(captured.def.id.as_str(), "$self::LIMIT");
        assert!(captured.slots.is_empty());
    }

    #[test]
    fn captures_an_interned_string_pool_declaration() {
        let captured = capture_interned_string_pool(quote::quote! {
            pub Browser {
                CHROME = "Chrome",
                FIREFOX = "Firefox",
            }
        })
        .expect("pool captures");

        assert_eq!(captured.name, "Browser");
        assert_eq!(captured.values, ["Chrome", "Firefox"]);
    }

    #[test]
    fn captures_interned_string_references_with_deferred_pools() {
        let item: syn::ItemFn = syn::parse_quote! {
            pub fn browser(name: InternedString<Browser>) -> u32 {
                unimplemented!()
            }
        };

        let captured = capture_function(&item).expect("function captures");

        assert_eq!(captured.slots.len(), 1, "the pool reference takes the slot");
        assert_eq!(
            captured.slots[0].to_token_stream().to_string(),
            "Browser",
            "the slot holds the pool's written path, not InternedString"
        );
        assert!(
            matches!(
                &captured.def.parameters[0].type_expr,
                TypeExpr::InternedString { pool_id, static_values, .. }
                    if pool_id == "$slot:0" && static_values.is_empty()
            ),
            "the pool defers to its slot with values left for aggregation"
        );
    }

    #[test]
    fn rejects_generics_loudly() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Wrapper<T> {
                pub inner: T,
            }
        };

        assert!(
            capture_struct(&item).is_err(),
            "generic items stay unsupported"
        );
    }

    #[test]
    fn rejects_qualified_class_impl_targets_loudly() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl crate::runtime::Engine {
                pub const LIMIT: u32 = 4;

                pub fn start(&self) {}
            }
        };

        let Err(error) = capture_class(&item, proc_macro2::TokenStream::new()) else {
            panic!("the invocation module cannot mint another module's class id");
        };
        assert!(error.to_string().contains("crate::runtime::Engine"));
        assert!(
            capture_class_constants(&item).is_err(),
            "constants refuse the same target"
        );
    }
}
