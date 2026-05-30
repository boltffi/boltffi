use askama::Template;

use super::plan::{
    JavaCallbackTrait, JavaClass, JavaClosureInterface, JavaEnum, JavaModule, JavaRecord,
};

pub fn javadoc_block(doc: &Option<String>, indent: &str) -> String {
    match doc {
        Some(text) => {
            let mut result = format!("{indent}/**\n");
            text.lines().for_each(|line| {
                if line.is_empty() {
                    result.push_str(&format!("{indent} *\n"));
                } else {
                    result.push_str(&format!("{indent} * {line}\n"));
                }
            });
            result.push_str(&format!("{indent} */\n"));
            result
        }
        None => String::new(),
    }
}

pub fn record_import_block(record: &JavaRecord) -> String {
    let mut imports = Vec::new();

    if !record.uses_native_record_syntax() {
        imports.push("import java.util.Objects;");
    }

    if record.is_blittable() {
        imports.extend([
            "import java.nio.ByteBuffer;",
            "import java.nio.ByteOrder;",
            "import java.util.ArrayList;",
            "import java.util.Collections;",
            "import java.util.List;",
        ]);
    }

    if imports.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", imports.join("\n"))
    }
}

#[derive(Template)]
#[template(path = "render_java/preamble.txt", escape = "none")]
pub struct PreambleTemplate<'a> {
    pub module: &'a JavaModule,
}

#[derive(Template)]
#[template(path = "render_java/result.txt", escape = "none")]
pub struct ResultTemplate<'a> {
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub record: &'a JavaRecord,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/native.txt", escape = "none")]
pub struct NativeTemplate<'a> {
    pub module: &'a JavaModule,
}

#[derive(Template)]
#[template(path = "render_java/functions.txt", escape = "none")]
pub struct FunctionsTemplate<'a> {
    pub module: &'a JavaModule,
}

#[derive(Template)]
#[template(path = "render_java/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate<'a> {
    pub enumeration: &'a JavaEnum,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/enum_error.txt", escape = "none")]
pub struct ErrorEnumTemplate<'a> {
    pub enumeration: &'a JavaEnum,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/enum_sealed.txt", escape = "none")]
pub struct DataEnumSealedTemplate<'a> {
    pub enumeration: &'a JavaEnum,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/enum_abstract.txt", escape = "none")]
pub struct DataEnumAbstractTemplate<'a> {
    pub enumeration: &'a JavaEnum,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/class.txt", escape = "none")]
pub struct ClassTemplate<'a> {
    pub class: &'a JavaClass,
    pub package_name: &'a str,
    pub async_mode: &'a super::plan::JavaAsyncMode,
}

#[derive(Template)]
#[template(path = "render_java/closure.txt", escape = "none")]
pub struct ClosureTemplate<'a> {
    pub closure: &'a JavaClosureInterface,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/callback_trait.txt", escape = "none")]
pub struct CallbackTraitTemplate<'a> {
    pub callback: &'a JavaCallbackTrait,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/closure_callbacks.txt", escape = "none")]
pub struct ClosureCallbacksTemplate<'a> {
    pub closure: &'a JavaClosureInterface,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/callback_callbacks.txt", escape = "none")]
pub struct CallbackCallbacksTemplate<'a> {
    pub callback: &'a JavaCallbackTrait,
    pub package_name: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::types::BuiltinKind;
    use crate::render::java::JavaVersion;
    use crate::render::java::plan::{
        JavaAsyncMode, JavaBuiltinSet, JavaClassMethod, JavaConstructor, JavaConstructorKind,
        JavaEnum, JavaEnumField, JavaEnumKind, JavaEnumVariant, JavaFunction, JavaInputBindings,
        JavaParam, JavaRecord, JavaRecordDefaultConstructor, JavaRecordDefaultConstructorParam,
        JavaRecordField, JavaReturnPlan, JavaReturnRender, JavaStream, JavaStreamMode,
        JavaWireWriter,
    };
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn java_param(name: &str, java_type: &str, native_type: &str, native_expr: &str) -> JavaParam {
        JavaParam {
            name: name.to_string(),
            java_type: java_type.to_string(),
            native_type: native_type.to_string(),
            native_expr: native_expr.to_string(),
        }
    }

    fn wire_writer(
        binding_name: &str,
        param_name: &str,
        size_expr: &str,
        encode_expr: &str,
    ) -> JavaWireWriter {
        JavaWireWriter {
            binding_name: binding_name.to_string(),
            param_name: param_name.to_string(),
            size_expr: size_expr.to_string(),
            encode_expr: encode_expr.to_string(),
        }
    }

    fn java_module(classes: Vec<JavaClass>) -> JavaModule {
        JavaModule {
            package_name: "com.test".to_string(),
            class_name: "Test".to_string(),
            lib_name: boltffi_ffi_rules::naming::library_name("test"),
            desktop_loader: true,
            java_version: JavaVersion::JAVA_17,
            async_mode: JavaAsyncMode::CompletableFuture,
            builtins: JavaBuiltinSet::default(),
            prefix: "boltffi".to_string(),
            records: vec![],
            enums: vec![],
            closures: vec![],
            callbacks: vec![],
            async_callback_invokers: vec![],
            functions: vec![],
            classes,
        }
    }

    #[test]
    fn result_template_deep_compares_array_payloads_when_java_available() {
        if Command::new("javac").arg("-version").output().is_err()
            || Command::new("java").arg("-version").output().is_err()
        {
            return;
        }

        let source = ResultTemplate {
            package_name: "com.test",
        }
        .render()
        .expect("result template should render");

        let smoke = r#"package com.test;

public final class ResultEqualitySmoke {
    public static void main(String[] args) {
        BoltFFIResult<byte[], String> leftBytes = BoltFFIResult.ok(new byte[] {1, 2, 3});
        BoltFFIResult<byte[], String> rightBytes = BoltFFIResult.ok(new byte[] {1, 2, 3});
        if (!leftBytes.equals(rightBytes)) throw new AssertionError("byte[] payloads should compare by contents");
        if (leftBytes.hashCode() != rightBytes.hashCode()) throw new AssertionError("equal byte[] payloads should hash equally");

        BoltFFIResult<int[], String> leftInts = BoltFFIResult.ok(new int[] {4, 5, 6});
        BoltFFIResult<int[], String> rightInts = BoltFFIResult.ok(new int[] {4, 5, 6});
        if (!leftInts.equals(rightInts)) throw new AssertionError("int[] payloads should compare by contents");
        if (leftInts.hashCode() != rightInts.hashCode()) throw new AssertionError("equal int[] payloads should hash equally");

        BoltFFIResult<java.util.List<int[]>, String> leftRows =
            BoltFFIResult.ok(java.util.Arrays.asList(new int[] {1, 2}, new int[] {3, 4}));
        BoltFFIResult<java.util.List<int[]>, String> rightRows =
            BoltFFIResult.ok(java.util.Arrays.asList(new int[] {1, 2}, new int[] {3, 4}));
        if (!leftRows.equals(rightRows)) throw new AssertionError("List<int[]> payloads should compare by nested contents");
        if (leftRows.hashCode() != rightRows.hashCode()) throw new AssertionError("equal List<int[]> payloads should hash equally");

        BoltFFIResult<java.util.Optional<byte[]>, String> leftOptional =
            BoltFFIResult.ok(java.util.Optional.of(new byte[] {7, 8}));
        BoltFFIResult<java.util.Optional<byte[]>, String> rightOptional =
            BoltFFIResult.ok(java.util.Optional.of(new byte[] {7, 8}));
        if (!leftOptional.equals(rightOptional)) throw new AssertionError("Optional<byte[]> payloads should compare by nested contents");
        if (leftOptional.hashCode() != rightOptional.hashCode()) throw new AssertionError("equal Optional<byte[]> payloads should hash equally");

        if (leftBytes.equals(BoltFFIResult.ok(new byte[] {1, 2, 4}))) throw new AssertionError("different arrays must not compare equal");
        if (leftBytes.equals(BoltFFIResult.err("boom"))) throw new AssertionError("Ok and Err must not compare equal");
    }
}
"#;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let tmp_root = std::env::temp_dir().join(format!("boltffi-java-result-eq-{nanos}"));
        let package_dir = tmp_root.join("com/test");
        let classes_dir = tmp_root.join("classes");
        fs::create_dir_all(&package_dir).expect("should create package directory");
        fs::create_dir_all(&classes_dir).expect("should create classes directory");

        let result_path = package_dir.join("BoltFFIResult.java");
        let smoke_path = package_dir.join("ResultEqualitySmoke.java");
        fs::write(&result_path, source).expect("should write result source");
        fs::write(&smoke_path, smoke).expect("should write smoke source");

        let javac = Command::new("javac")
            .arg("-d")
            .arg(&classes_dir)
            .arg(&result_path)
            .arg(&smoke_path)
            .output()
            .expect("javac should execute");
        assert!(
            javac.status.success(),
            "result equality smoke sources should compile: {}",
            String::from_utf8_lossy(&javac.stderr)
        );

        let java = Command::new("java")
            .arg("-cp")
            .arg(&classes_dir)
            .arg("com.test.ResultEqualitySmoke")
            .output()
            .expect("java should execute");
        assert!(
            java.status.success(),
            "result equality smoke should pass: {}",
            String::from_utf8_lossy(&java.stderr)
        );

        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn preamble_omits_builtin_runtime_when_module_has_no_builtins() {
        let module = java_module(vec![]);

        let source = PreambleTemplate { module: &module }
            .render()
            .expect("preamble template should render");

        assert!(!source.contains("import java.net.URI;"));
        assert!(!source.contains("import java.time.Duration;"));
        assert!(!source.contains("import java.time.Instant;"));
        assert!(!source.contains("import java.util.UUID;"));
        assert!(!source.contains("readDuration()"));
        assert!(!source.contains("readInstant()"));
        assert!(!source.contains("readUuid()"));
        assert!(!source.contains("readUri()"));
        assert!(!source.contains("writeDuration("));
        assert!(!source.contains("writeInstant("));
        assert!(!source.contains("writeUuid("));
        assert!(!source.contains("writeUri("));
    }

    #[test]
    fn preamble_emits_only_fully_qualified_builtin_runtime_when_builtins_are_used() {
        let mut module = java_module(vec![]);
        module.builtins = JavaBuiltinSet::from_kinds(
            [
                BuiltinKind::Duration,
                BuiltinKind::SystemTime,
                BuiltinKind::Uuid,
                BuiltinKind::Url,
            ]
            .into_iter()
            .collect(),
        );
        module.functions = vec![JavaFunction {
            doc: None,
            name: "storeDuration".to_string(),
            ffi_name: "boltffi_store_duration".to_string(),
            params: vec![],
            return_type: "void".to_string(),
            return_plan: JavaReturnPlan {
                native_return_type: "void".to_string(),
                render: JavaReturnRender::Void,
            },
            input_bindings: JavaInputBindings {
                direct_composites: vec![],
                wire_writers: vec![wire_writer(
                    "_wireDuration",
                    "duration",
                    "WireWriter.sizeOfDuration(duration)",
                    "writer.writeDuration(duration)",
                )],
            },
            async_call: None,
        }];

        let source = PreambleTemplate { module: &module }
            .render()
            .expect("preamble template should render");

        assert!(!source.contains("import java.net.URI;"));
        assert!(!source.contains("import java.time.Duration;"));
        assert!(!source.contains("import java.time.Instant;"));
        assert!(!source.contains("import java.util.UUID;"));
        assert!(source.contains("java.time.Duration readDuration()"));
        assert!(source.contains("java.time.Instant readInstant()"));
        assert!(source.contains("java.util.UUID readUuid()"));
        assert!(source.contains("java.net.URI readUri()"));
        assert!(source.contains("void writeDuration(java.time.Duration value)"));
        assert!(source.contains("void writeInstant(java.time.Instant value)"));
        assert!(source.contains("void writeUuid(java.util.UUID value)"));
        assert!(source.contains("void writeUri(java.net.URI value)"));
    }

    #[test]
    fn class_template_renders_nullable_handle_return_guard() {
        let class = JavaClass {
            doc: None,
            class_name: "Node".to_string(),
            ffi_free: "boltffi_node_free".to_string(),
            constructors: vec![],
            methods: vec![JavaClassMethod {
                doc: None,
                name: "maybeNext".to_string(),
                ffi_name: "boltffi_node_maybe_next".to_string(),
                is_static: false,
                params: vec![],
                return_type: "Node".to_string(),
                return_plan: JavaReturnPlan {
                    native_return_type: "long".to_string(),
                    render: JavaReturnRender::Handle {
                        class_name: "Node".to_string(),
                        nullable: true,
                    },
                },
                input_bindings: JavaInputBindings::default(),
                async_call: None,
            }],
            streams: vec![],
        };

        let source = ClassTemplate {
            class: &class,
            package_name: "com.test",
            async_mode: &JavaAsyncMode::CompletableFuture,
        }
        .render()
        .expect("class template should render");

        assert!(source.contains("if (_handle == 0L) return null;"));
        assert!(source.contains("return new Node(_handle);"));
    }

    #[test]
    fn class_template_renders_c_style_enum_decode_for_wire_methods() {
        let payload_param = java_param(
            "payload",
            "Payload",
            "ByteBuffer",
            "_wire_payload.toBuffer()",
        );
        let payload_writer = wire_writer(
            "_wire_payload",
            "payload",
            "8",
            "encodePayload(_wire_payload)",
        );

        let class = JavaClass {
            doc: None,
            class_name: "Counter".to_string(),
            ffi_free: "boltffi_counter_free".to_string(),
            constructors: vec![],
            methods: vec![
                JavaClassMethod {
                    doc: None,
                    name: "fromPayload".to_string(),
                    ffi_name: "boltffi_counter_from_payload".to_string(),
                    is_static: true,
                    params: vec![payload_param.clone()],
                    return_type: "Status".to_string(),
                    return_plan: JavaReturnPlan {
                        native_return_type: "int".to_string(),
                        render: JavaReturnRender::CStyleEnum {
                            class_name: "Status".to_string(),
                        },
                    },
                    input_bindings: JavaInputBindings {
                        direct_composites: vec![],
                        wire_writers: vec![payload_writer.clone()],
                    },
                    async_call: None,
                },
                JavaClassMethod {
                    doc: None,
                    name: "stateWithPayload".to_string(),
                    ffi_name: "boltffi_counter_state_with_payload".to_string(),
                    is_static: false,
                    params: vec![payload_param],
                    return_type: "Status".to_string(),
                    return_plan: JavaReturnPlan {
                        native_return_type: "int".to_string(),
                        render: JavaReturnRender::CStyleEnum {
                            class_name: "Status".to_string(),
                        },
                    },
                    input_bindings: JavaInputBindings {
                        direct_composites: vec![],
                        wire_writers: vec![payload_writer],
                    },
                    async_call: None,
                },
            ],
            streams: vec![],
        };

        let source = ClassTemplate {
            class: &class,
            package_name: "com.test",
            async_mode: &JavaAsyncMode::CompletableFuture,
        }
        .render()
        .expect("class template should render");

        assert!(
            source.contains(
                "return Status.fromValue(Native.boltffi_counter_from_payload(_wire_payload.toBuffer()));"
            )
        );
        assert!(
            source.contains(
                "return Status.fromValue(Native.boltffi_counter_state_with_payload(handle, _wire_payload.toBuffer()));"
            )
        );
    }

    #[test]
    fn native_template_renders_class_native_declarations() {
        let class = JavaClass {
            doc: None,
            class_name: "Counter".to_string(),
            ffi_free: "boltffi_counter_free".to_string(),
            constructors: vec![JavaConstructor {
                doc: None,
                kind: JavaConstructorKind::Primary,
                name: String::new(),
                is_fallible: false,
                params: vec![],
                ffi_name: "boltffi_counter_new".to_string(),
                input_bindings: JavaInputBindings::default(),
            }],
            methods: vec![
                JavaClassMethod {
                    doc: None,
                    name: "globalCount".to_string(),
                    ffi_name: "boltffi_counter_global_count".to_string(),
                    is_static: true,
                    params: vec![],
                    return_type: "int".to_string(),
                    return_plan: JavaReturnPlan {
                        native_return_type: "int".to_string(),
                        render: JavaReturnRender::Direct,
                    },
                    input_bindings: JavaInputBindings::default(),
                    async_call: None,
                },
                JavaClassMethod {
                    doc: None,
                    name: "get".to_string(),
                    ffi_name: "boltffi_counter_get".to_string(),
                    is_static: false,
                    params: vec![],
                    return_type: "int".to_string(),
                    return_plan: JavaReturnPlan {
                        native_return_type: "int".to_string(),
                        render: JavaReturnRender::Direct,
                    },
                    input_bindings: JavaInputBindings::default(),
                    async_call: None,
                },
            ],
            streams: vec![],
        };
        let module = JavaModule {
            functions: vec![JavaFunction {
                doc: None,
                name: "noop".to_string(),
                ffi_name: "boltffi_noop".to_string(),
                params: vec![],
                return_type: "void".to_string(),
                return_plan: JavaReturnPlan {
                    native_return_type: "void".to_string(),
                    render: JavaReturnRender::Void,
                },
                input_bindings: JavaInputBindings::default(),
                async_call: None,
            }],
            ..java_module(vec![class])
        };

        let source = NativeTemplate { module: &module }
            .render()
            .expect("native template should render");

        assert!(source.contains("loadDesktopLibraries(preferredLibrary, fallbackLibrary);"));
        assert!(source.contains(
            "UnsatisfiedLinkError preferredFailure = tryLoadDesktopLibrary(preferredLibrary);"
        ));
        assert!(source.contains("if (tryLoadOptionalDesktopLibrary(fallbackLibrary)) {"));
        assert!(source.contains(
            "private static UnsatisfiedLinkError tryLoadDesktopLibrary(String libraryName) {"
        ));
        assert!(
            source.contains(
                "private static boolean loadExternalLibraryIfPresent(String libraryName) {"
            )
        );
        assert!(source.contains("System.load(extracted.getAbsolutePath());"));
        assert!(source.contains("bundledLibraryResourceCandidates"));
        assert!(!source.contains("java.nio.file"));
        assert!(source.contains("static native void boltffi_counter_free(long handle);"));
        assert!(source.contains("static native long boltffi_counter_new();"));
        assert!(source.contains("static native int boltffi_counter_global_count();"));
        assert!(source.contains("static native int boltffi_counter_get(long handle);"));
    }

    #[test]
    fn native_template_accepts_linux_amd64_for_bundled_desktop_loading() {
        let source = NativeTemplate {
            module: &java_module(vec![]),
        }
        .render()
        .expect("native template should render");

        assert!(source.contains(
            "if (osName.contains(\"linux\") && (osArch.equals(\"x86_64\") || osArch.equals(\"amd64\")))"
        ));
    }

    #[test]
    fn native_template_omits_desktop_loader_for_android_bindings() {
        let mut module = java_module(vec![]);
        module.desktop_loader = false;

        let source = NativeTemplate { module: &module }
            .render()
            .expect("native template should render");

        assert!(source.contains("System.loadLibrary(fallbackLibrary);"));
        assert!(!source.contains("loadDesktopLibraries(preferredLibrary, fallbackLibrary);"));
        assert!(!source.contains("bundledLibraryResourceCandidates"));
    }

    #[test]
    fn class_template_uses_single_stream_subscription_for_all_stream_modes() {
        let class = JavaClass {
            doc: None,
            class_name: "EventBus".to_string(),
            ffi_free: "boltffi_event_bus_free".to_string(),
            constructors: vec![],
            methods: vec![],
            streams: vec![
                JavaStream {
                    doc: None,
                    name: "subscribeValues".to_string(),
                    item_type: "Integer".to_string(),
                    pop_batch_items_expr: "WireReader.readPackedInts(_bytes)".to_string(),
                    subscribe: "boltffi_event_bus_subscribe_values".to_string(),
                    poll: "boltffi_event_bus_subscribe_values_poll".to_string(),
                    pop_batch: "boltffi_event_bus_subscribe_values_pop_batch".to_string(),
                    wait: "boltffi_event_bus_subscribe_values_wait".to_string(),
                    unsubscribe: "boltffi_event_bus_subscribe_values_unsubscribe".to_string(),
                    free: "boltffi_event_bus_subscribe_values_free".to_string(),
                    mode: JavaStreamMode::Async,
                },
                JavaStream {
                    doc: None,
                    name: "subscribeValuesBatch".to_string(),
                    item_type: "Integer".to_string(),
                    pop_batch_items_expr: "WireReader.readPackedInts(_bytes)".to_string(),
                    subscribe: "boltffi_event_bus_subscribe_values_batch".to_string(),
                    poll: "boltffi_event_bus_subscribe_values_batch_poll".to_string(),
                    pop_batch: "boltffi_event_bus_subscribe_values_batch_pop_batch".to_string(),
                    wait: "boltffi_event_bus_subscribe_values_batch_wait".to_string(),
                    unsubscribe: "boltffi_event_bus_subscribe_values_batch_unsubscribe".to_string(),
                    free: "boltffi_event_bus_subscribe_values_batch_free".to_string(),
                    mode: JavaStreamMode::Batch,
                },
                JavaStream {
                    doc: None,
                    name: "subscribeValuesCallback".to_string(),
                    item_type: "Integer".to_string(),
                    pop_batch_items_expr: "WireReader.readPackedInts(_bytes)".to_string(),
                    subscribe: "boltffi_event_bus_subscribe_values_callback".to_string(),
                    poll: "boltffi_event_bus_subscribe_values_callback_poll".to_string(),
                    pop_batch: "boltffi_event_bus_subscribe_values_callback_pop_batch".to_string(),
                    wait: "boltffi_event_bus_subscribe_values_callback_wait".to_string(),
                    unsubscribe: "boltffi_event_bus_subscribe_values_callback_unsubscribe"
                        .to_string(),
                    free: "boltffi_event_bus_subscribe_values_callback_free".to_string(),
                    mode: JavaStreamMode::Callback,
                },
            ],
        };

        let source = ClassTemplate {
            class: &class,
            package_name: "com.test",
            async_mode: &JavaAsyncMode::CompletableFuture,
        }
        .render()
        .expect("class template should render");

        assert!(source.contains("public StreamSubscription<Integer> subscribeValues(java.util.function.Consumer<Integer> callback)"));
        assert!(source.contains("public StreamSubscription<Integer> subscribeValuesBatch()"));
        assert!(source.contains("public StreamSubscription<Integer> subscribeValuesCallback(java.util.function.Consumer<Integer> callback)"));
    }

    #[test]
    fn preamble_template_renders_live_stream_publisher() {
        let class = JavaClass {
            doc: None,
            class_name: "EventBus".to_string(),
            ffi_free: "boltffi_event_bus_free".to_string(),
            constructors: vec![],
            methods: vec![],
            streams: vec![JavaStream {
                doc: None,
                name: "subscribeValuesBatch".to_string(),
                item_type: "Integer".to_string(),
                pop_batch_items_expr: "WireReader.readPackedInts(_bytes)".to_string(),
                subscribe: "boltffi_event_bus_subscribe_values_batch".to_string(),
                poll: "boltffi_event_bus_subscribe_values_batch_poll".to_string(),
                pop_batch: "boltffi_event_bus_subscribe_values_batch_pop_batch".to_string(),
                wait: "boltffi_event_bus_subscribe_values_batch_wait".to_string(),
                unsubscribe: "boltffi_event_bus_subscribe_values_batch_unsubscribe".to_string(),
                free: "boltffi_event_bus_subscribe_values_batch_free".to_string(),
                mode: JavaStreamMode::Batch,
            }],
        };
        let module = java_module(vec![class]);

        let source = PreambleTemplate { module: &module }
            .render()
            .expect("preamble template should render");

        assert!(source.contains("final class StreamSubscription<T> implements AutoCloseable"));
        assert!(
            source.contains("static <T> StreamSubscription<T> callback(Runnable cancelAction)")
        );
        assert!(source.contains("static <T> StreamSubscription<T> batch("));
        assert!(source.contains("requireBatchMode(\"toPublisher\")"));
        assert!(source.contains("if (!publisherAttached.compareAndSet(false, true))"));
        assert!(source.contains("int waitResult = waitFn.apply(handle, WAIT_TIMEOUT_MILLIS);"));
        assert!(source.contains("subscriber.onComplete();"));
    }

    #[test]
    fn class_template_renders_doc_comments() {
        let class = JavaClass {
            doc: Some("A data store.\nPersists values across calls.".to_string()),
            class_name: "Store".to_string(),
            ffi_free: "boltffi_store_free".to_string(),
            constructors: vec![JavaConstructor {
                doc: Some("Creates a store with the requested capacity.".to_string()),
                kind: JavaConstructorKind::Primary,
                name: String::new(),
                is_fallible: false,
                params: vec![java_param("capacity", "int", "int", "capacity")],
                ffi_name: "boltffi_store_new".to_string(),
                input_bindings: JavaInputBindings::default(),
            }],
            methods: vec![JavaClassMethod {
                doc: Some("Returns the number of stored items.".to_string()),
                name: "count".to_string(),
                ffi_name: "boltffi_store_count".to_string(),
                is_static: false,
                params: vec![],
                return_type: "int".to_string(),
                return_plan: JavaReturnPlan {
                    native_return_type: "int".to_string(),
                    render: JavaReturnRender::Direct,
                },
                input_bindings: JavaInputBindings::default(),
                async_call: None,
            }],
            streams: vec![JavaStream {
                doc: Some("Subscribes to value changes.".to_string()),
                name: "subscribeValuesBatch".to_string(),
                item_type: "Integer".to_string(),
                pop_batch_items_expr: "WireReader.readPackedInts(_bytes)".to_string(),
                subscribe: "boltffi_store_subscribe_values".to_string(),
                poll: "boltffi_store_subscribe_values_poll".to_string(),
                pop_batch: "boltffi_store_subscribe_values_pop_batch".to_string(),
                wait: "boltffi_store_subscribe_values_wait".to_string(),
                unsubscribe: "boltffi_store_subscribe_values_unsubscribe".to_string(),
                free: "boltffi_store_subscribe_values_free".to_string(),
                mode: JavaStreamMode::Batch,
            }],
        };

        let source = ClassTemplate {
            class: &class,
            package_name: "com.test",
            async_mode: &JavaAsyncMode::CompletableFuture,
        }
        .render()
        .expect("class template should render");

        assert!(source.contains(
            "/**\n * A data store.\n * Persists values across calls.\n */\npublic final class Store"
        ));
        assert!(source.contains("/**\n     * Creates a store with the requested capacity.\n     */\n    public Store(int capacity)"));
        assert!(source.contains(
            "/**\n     * Returns the number of stored items.\n     */\n    public int count()"
        ));
        assert!(source.contains("/**\n     * Subscribes to value changes.\n     */\n    public StreamSubscription<Integer> subscribeValuesBatch()"));
    }

    #[test]
    fn record_and_enum_templates_render_doc_comments() {
        let record = JavaRecord {
            doc: Some("A physical point.".to_string()),
            shape: crate::render::java::plan::JavaRecordShape::ClassicClass,
            class_name: "Point".to_string(),
            is_error: false,
            fields: vec![JavaRecordField {
                doc: Some("Horizontal coordinate.".to_string()),
                name: "x".to_string(),
                java_type: "double".to_string(),
                default_value: None,
                wire_decode_expr: "reader.readF64()".to_string(),
                wire_size_expr: "8".to_string(),
                wire_encode_expr: "wire.writeF64(x)".to_string(),
                equals_expr: "Double.compare(x, other.x) == 0".to_string(),
                hash_expr: "Double.hashCode(x)".to_string(),
            }],
            default_constructors: vec![],
            blittable_layout: None,
            constructors: vec![],
            methods: vec![],
        };

        let record_source = RecordTemplate {
            record: &record,
            package_name: "com.test",
        }
        .render()
        .expect("record template should render");

        assert!(record_source.contains("/**\n * A physical point.\n */\npublic final class Point"));
        assert!(
            record_source
                .contains("/**\n     * Horizontal coordinate.\n     */\n    public double x()")
        );
        assert!(record_source.contains("package com.test;\n\nimport java.util.Objects;\n\n/**"));

        let enumeration = JavaEnum {
            doc: Some("Represents a direction.".to_string()),
            class_name: "Direction".to_string(),
            kind: JavaEnumKind::CStyle,
            value_type: "int".to_string(),
            variants: vec![JavaEnumVariant {
                doc: Some("Points north.".to_string()),
                name: "NORTH".to_string(),
                tag: 0,
                fields: vec![],
            }],
            constructors: vec![],
            methods: vec![],
        };

        let enum_source = CStyleEnumTemplate {
            enumeration: &enumeration,
            package_name: "com.test",
        }
        .render()
        .expect("enum template should render");

        assert!(
            enum_source.contains("/**\n * Represents a direction.\n */\npublic enum Direction")
        );
        assert!(enum_source.contains("/**\n     * Points north.\n     */\n    NORTH(0);"));
    }

    #[test]
    fn record_template_renders_default_field_overloads() {
        let record = JavaRecord {
            doc: None,
            shape: crate::render::java::plan::JavaRecordShape::NativeRecord,
            is_error: false,
            class_name: "Config".to_string(),
            fields: vec![
                JavaRecordField {
                    doc: None,
                    name: "name".to_string(),
                    java_type: "String".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readString()".to_string(),
                    wire_size_expr: "4".to_string(),
                    wire_encode_expr: "wire.writeString(name)".to_string(),
                    equals_expr: "java.util.Objects.equals(name, other.name)".to_string(),
                    hash_expr: "java.util.Objects.hashCode(name)".to_string(),
                },
                JavaRecordField {
                    doc: None,
                    name: "retries".to_string(),
                    java_type: "int".to_string(),
                    default_value: Some("3".to_string()),
                    wire_decode_expr: "reader.readI32()".to_string(),
                    wire_size_expr: "4".to_string(),
                    wire_encode_expr: "wire.writeI32(retries)".to_string(),
                    equals_expr: "retries == other.retries".to_string(),
                    hash_expr: "Integer.hashCode(retries)".to_string(),
                },
                JavaRecordField {
                    doc: None,
                    name: "label".to_string(),
                    java_type: "java.util.Optional<String>".to_string(),
                    default_value: Some("java.util.Optional.empty()".to_string()),
                    wire_decode_expr: "reader.readOptionalString()".to_string(),
                    wire_size_expr: "1".to_string(),
                    wire_encode_expr: "wire.writeOptionalString(label)".to_string(),
                    equals_expr: "java.util.Objects.equals(label, other.label)".to_string(),
                    hash_expr: "java.util.Objects.hashCode(label)".to_string(),
                },
                JavaRecordField {
                    doc: None,
                    name: "alias".to_string(),
                    java_type: "java.util.Optional<String>".to_string(),
                    default_value: Some("java.util.Optional.of(\"primary\")".to_string()),
                    wire_decode_expr: "reader.readOptionalString()".to_string(),
                    wire_size_expr: "1".to_string(),
                    wire_encode_expr: "wire.writeOptionalString(alias)".to_string(),
                    equals_expr: "java.util.Objects.equals(alias, other.alias)".to_string(),
                    hash_expr: "java.util.Objects.hashCode(alias)".to_string(),
                },
            ],
            default_constructors: vec![
                JavaRecordDefaultConstructor {
                    params: vec![
                        JavaRecordDefaultConstructorParam {
                            name: "name".to_string(),
                            java_type: "String".to_string(),
                        },
                        JavaRecordDefaultConstructorParam {
                            name: "retries".to_string(),
                            java_type: "int".to_string(),
                        },
                        JavaRecordDefaultConstructorParam {
                            name: "label".to_string(),
                            java_type: "java.util.Optional<String>".to_string(),
                        },
                    ],
                    arguments: vec![
                        "name".to_string(),
                        "retries".to_string(),
                        "label".to_string(),
                        "java.util.Optional.of(\"primary\")".to_string(),
                    ],
                },
                JavaRecordDefaultConstructor {
                    arguments: vec![
                        "name".to_string(),
                        "retries".to_string(),
                        "java.util.Optional.empty()".to_string(),
                        "java.util.Optional.of(\"primary\")".to_string(),
                    ],
                    params: vec![
                        JavaRecordDefaultConstructorParam {
                            name: "name".to_string(),
                            java_type: "String".to_string(),
                        },
                        JavaRecordDefaultConstructorParam {
                            name: "retries".to_string(),
                            java_type: "int".to_string(),
                        },
                    ],
                },
                JavaRecordDefaultConstructor {
                    params: vec![JavaRecordDefaultConstructorParam {
                        name: "name".to_string(),
                        java_type: "String".to_string(),
                    }],
                    arguments: vec![
                        "name".to_string(),
                        "3".to_string(),
                        "java.util.Optional.empty()".to_string(),
                        "java.util.Optional.of(\"primary\")".to_string(),
                    ],
                },
            ],
            blittable_layout: None,
            constructors: vec![],
            methods: vec![],
        };

        let source = RecordTemplate {
            record: &record,
            package_name: "com.test",
        }
        .render()
        .expect("record template should render");

        assert!(source.contains(
            "public record Config(String name, int retries, java.util.Optional<String> label, java.util.Optional<String> alias)"
        ));
        assert!(source.contains(
            "public Config(String name, int retries, java.util.Optional<String> label) {\n        this(name, retries, label, java.util.Optional.of(\"primary\"));\n    }"
        ));
        assert!(source.contains(
            "public Config(String name, int retries) {\n        this(name, retries, java.util.Optional.empty(), java.util.Optional.of(\"primary\"));\n    }"
        ));
        assert!(source.contains(
            "public Config(String name) {\n        this(name, 3, java.util.Optional.empty(), java.util.Optional.of(\"primary\"));\n    }"
        ));
    }

    #[test]
    fn abstract_error_enum_template_preserves_payloads() {
        let enumeration = JavaEnum {
            doc: None,
            class_name: "ApiError".to_string(),
            kind: JavaEnumKind::ErrorAbstractClass,
            value_type: "int".to_string(),
            variants: vec![
                JavaEnumVariant {
                    doc: None,
                    name: "Network".to_string(),
                    tag: 0,
                    fields: vec![JavaEnumField {
                        doc: None,
                        name: "message".to_string(),
                        java_type: "String".to_string(),
                        wire_decode_expr: "reader.readString()".to_string(),
                        wire_size_expr: "WireWriter.sizeString(_v.message)".to_string(),
                        wire_encode_expr: "wire.writeString(_v.message)".to_string(),
                        equals_expr: "java.util.Objects.equals(this.message, other.message)"
                            .to_string(),
                        hash_expr: "java.util.Objects.hashCode(message)".to_string(),
                    }],
                },
                JavaEnumVariant {
                    doc: None,
                    name: "Timeout".to_string(),
                    tag: 1,
                    fields: vec![],
                },
            ],
            constructors: vec![],
            methods: vec![],
        };

        let source = DataEnumAbstractTemplate {
            enumeration: &enumeration,
            package_name: "com.test",
        }
        .render()
        .expect("abstract error enum template should render");

        assert!(source.contains("public abstract class ApiError extends RuntimeException"));
        assert!(source.contains("protected ApiError(String message)"));
        assert!(source.contains("public final String message;"));
        assert!(source.contains("super(message);"));
        assert!(source.contains("private Timeout() {"));
        assert!(source.contains("super(\"ApiError.Timeout\");"));
    }
}
