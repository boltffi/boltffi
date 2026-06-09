use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ir::definitions::{EnumRepr, VariantPayload};
use crate::ir::{self, AbiContract, FfiContract};
use crate::render::jni::{JniEmitter, JniLowerer, JvmBindingStyle};
use crate::render::kotlin::{KotlinEmitter, KotlinLowerer, KotlinOptions, NamingConvention};

const KMP_COMMON_RUNTIME_TYPE_NAMES: &[&str] = &["FfiException", "BoltFFIResult"];

#[derive(Debug, Clone)]
pub struct KMPOptions {
    pub package_name: String,
    pub module_name: String,
    pub min_sdk: u32,
    pub kotlin_options: KotlinOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KMPOutputFile {
    pub relative_path: PathBuf,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KMPOutput {
    pub files: Vec<KMPOutputFile>,
}

pub struct KMPEmitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KmpActualBackend {
    KotlinJvm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KmpPlatformAdapter {
    source_set: &'static str,
    actual_file_suffix: &'static str,
    backend: KmpActualBackend,
}

impl KmpPlatformAdapter {
    const fn jvm() -> Self {
        Self {
            source_set: "jvmMain",
            actual_file_suffix: "JvmActual",
            backend: KmpActualBackend::KotlinJvm,
        }
    }

    const fn android() -> Self {
        Self {
            source_set: "androidMain",
            actual_file_suffix: "AndroidActual",
            backend: KmpActualBackend::KotlinJvm,
        }
    }
}

struct KmpRender {
    common: String,
    platform_actuals: Vec<KmpPlatformActual>,
}

struct KmpPlatformActual {
    adapter: KmpPlatformAdapter,
    contents: String,
}

struct KmpSurfaceSupport {
    records: HashSet<String>,
    enums: HashSet<String>,
    custom_types: HashSet<String>,
}

struct KmpEnumVariant {
    name: String,
    payload: VariantPayload,
    doc: Option<String>,
}

struct KmpEnumField {
    name: String,
    type_expr: ir::types::TypeExpr,
}

impl KmpEnumField {
    fn kotlin_name(&self) -> String {
        self.name.clone()
    }
}

impl KmpSurfaceSupport {
    fn for_contract(contract: &ir::FfiContract) -> Self {
        let mut records = HashSet::new();
        let mut enums = HashSet::new();
        let mut custom_types = HashSet::new();

        loop {
            let before = records.len() + enums.len() + custom_types.len();

            contract.catalog.all_enums().for_each(|enumeration| {
                if enum_supported_with_sets(enumeration, contract, &records, &enums, &custom_types)
                {
                    enums.insert(enumeration.id.as_str().to_string());
                }
            });

            contract.catalog.all_records().for_each(|record| {
                if record_supported_with_sets(record, contract, &records, &enums, &custom_types) {
                    records.insert(record.id.as_str().to_string());
                }
            });

            contract.catalog.all_custom_types().for_each(|custom| {
                if custom_type_supported_with_sets(
                    custom,
                    contract,
                    &records,
                    &enums,
                    &custom_types,
                ) {
                    custom_types.insert(custom.id.as_str().to_string());
                }
            });

            if records.len() + enums.len() + custom_types.len() == before {
                break;
            }
        }

        Self {
            records,
            enums,
            custom_types,
        }
    }
}

impl KMPEmitter {
    fn package_path(package_name: &str) -> PathBuf {
        package_name.split('.').collect()
    }

    pub fn emit(contract: &FfiContract, abi: &AbiContract, options: KMPOptions) -> KMPOutput {
        let KMPOptions {
            package_name,
            module_name,
            min_sdk,
            kotlin_options,
        } = options;
        let internal_package = format!("{package_name}.jvm");
        let common_package_path = Self::package_path(&package_name);
        let internal_package_path = Self::package_path(&internal_package);
        let platform_adapters = Self::default_platform_adapters();
        let support = KmpSurfaceSupport::for_contract(contract);

        let rendered = Self::render_surfaces_with_support(
            contract,
            &package_name,
            &internal_package,
            &platform_adapters,
            &support,
        );
        let internal_contract = filter_contract_for_kmp_surface(contract, &support);
        let internal_abi = filter_abi_for_kmp_surface(contract, abi, &support);

        let kotlin_module = KotlinLowerer::new(
            &internal_contract,
            &internal_abi,
            internal_package.clone(),
            module_name.clone(),
            kotlin_options,
        )
        .lower();
        let jvm_source = KotlinEmitter::emit(&kotlin_module);

        let jni_module = JniLowerer::new(
            &internal_contract,
            &internal_abi,
            internal_package,
            module_name.clone(),
        )
        .with_jvm_binding_style(JvmBindingStyle::Kotlin)
        .lower();
        let jni_source = JniEmitter::emit(&jni_module);

        let common_dir = PathBuf::from("src/commonMain/kotlin").join(&common_package_path);

        let mut files = vec![
            KMPOutputFile {
                relative_path: PathBuf::from("settings.gradle.kts"),
                contents: Self::render_settings_gradle(&module_name),
            },
            KMPOutputFile {
                relative_path: PathBuf::from("build.gradle.kts"),
                contents: Self::render_build_gradle(&package_name, min_sdk),
            },
            KMPOutputFile {
                relative_path: common_dir.join(format!("{module_name}.kt")),
                contents: rendered.common,
            },
        ];

        rendered.platform_actuals.into_iter().for_each(|actual| {
            let actual_dir =
                Self::source_set_kotlin_dir(actual.adapter.source_set, &common_package_path);
            files.push(KMPOutputFile {
                relative_path: actual_dir.join(format!(
                    "{}{}.kt",
                    module_name, actual.adapter.actual_file_suffix
                )),
                contents: actual.contents,
            });
        });

        platform_adapters
            .iter()
            .filter(|adapter| matches!(adapter.backend, KmpActualBackend::KotlinJvm))
            .for_each(|adapter| {
                let internal_dir =
                    Self::source_set_kotlin_dir(adapter.source_set, &internal_package_path);
                files.push(KMPOutputFile {
                    relative_path: internal_dir.join(format!("{module_name}.kt")),
                    contents: jvm_source.clone(),
                });
            });

        platform_adapters
            .iter()
            .filter(|adapter| matches!(adapter.backend, KmpActualBackend::KotlinJvm))
            .for_each(|adapter| {
                files.push(KMPOutputFile {
                    relative_path: PathBuf::from(format!(
                        "src/{}/c/jni_glue.c",
                        adapter.source_set
                    )),
                    contents: jni_source.clone(),
                });
            });

        KMPOutput { files }
    }

    fn default_platform_adapters() -> Vec<KmpPlatformAdapter> {
        vec![KmpPlatformAdapter::jvm(), KmpPlatformAdapter::android()]
    }

    fn source_set_kotlin_dir(source_set: &str, package_path: &Path) -> PathBuf {
        PathBuf::from(format!("src/{source_set}/kotlin")).join(package_path)
    }

    fn render_surfaces(
        contract: &ir::FfiContract,
        package_name: &str,
        internal_package: &str,
        platform_adapters: &[KmpPlatformAdapter],
    ) -> KmpRender {
        let support = KmpSurfaceSupport::for_contract(contract);
        Self::render_surfaces_with_support(
            contract,
            package_name,
            internal_package,
            platform_adapters,
            &support,
        )
    }

    fn render_surfaces_with_support(
        contract: &ir::FfiContract,
        package_name: &str,
        internal_package: &str,
        platform_adapters: &[KmpPlatformAdapter],
        support: &KmpSurfaceSupport,
    ) -> KmpRender {
        let common = Self::render_common_surface(contract, package_name, support);
        let platform_actuals = platform_adapters
            .iter()
            .map(|adapter| KmpPlatformActual {
                adapter: *adapter,
                contents: Self::render_platform_actual(
                    contract,
                    package_name,
                    internal_package,
                    support,
                    *adapter,
                ),
            })
            .collect();

        KmpRender {
            common,
            platform_actuals,
        }
    }

    fn render_common_surface(
        contract: &ir::FfiContract,
        package_name: &str,
        support: &KmpSurfaceSupport,
    ) -> String {
        let mut common_sections = Vec::new();
        common_sections.push("// Auto-generated by BoltFFI. Do not edit.".to_string());
        common_sections.push(format!("package {package_name}"));
        common_sections.push(Self::render_common_result_runtime());

        let mut unsupported = Vec::new();

        contract
            .catalog
            .all_custom_types()
            .filter(|custom| support.custom_types.contains(custom.id.as_str()))
            .map(Self::render_custom_type)
            .for_each(|section| common_sections.push(section));

        contract
            .catalog
            .all_records()
            .filter(|record| support.records.contains(record.id.as_str()))
            .map(|record| Self::render_common_record(record, contract))
            .for_each(|section| common_sections.push(section));

        contract
            .catalog
            .all_enums()
            .filter(|enumeration| support.enums.contains(enumeration.id.as_str()))
            .map(|enumeration| Self::render_common_enum(enumeration, package_name, contract))
            .for_each(|section| common_sections.push(section));

        contract.functions.iter().for_each(|function| {
            if function_supported(function, contract, support) {
                common_sections.push(Self::render_common_function(function));
            } else {
                unsupported.push(function.id.as_str().to_string());
            }
        });

        if !unsupported.is_empty() {
            common_sections.push(format!(
                "// Unsupported in the initial KMP generator slice: {}",
                unsupported.join(", ")
            ));
        }

        join_kotlin_sections(common_sections)
    }

    fn render_platform_actual(
        contract: &ir::FfiContract,
        package_name: &str,
        internal_package: &str,
        support: &KmpSurfaceSupport,
        adapter: KmpPlatformAdapter,
    ) -> String {
        match adapter.backend {
            KmpActualBackend::KotlinJvm => {
                Self::render_kotlin_jvm_actual(contract, package_name, internal_package, support)
            }
        }
    }

    fn render_kotlin_jvm_actual(
        contract: &ir::FfiContract,
        package_name: &str,
        internal_package: &str,
        support: &KmpSurfaceSupport,
    ) -> String {
        let mut actual_sections = Vec::new();
        actual_sections.push("// Auto-generated by BoltFFI. Do not edit.".to_string());
        actual_sections.push(format!("package {package_name}"));

        contract
            .catalog
            .all_records()
            .filter(|record| support.records.contains(record.id.as_str()))
            .map(|record| {
                Self::render_record_actual_conversions(record, internal_package, contract)
            })
            .for_each(|section| actual_sections.push(section));

        contract
            .catalog
            .all_enums()
            .filter(|enumeration| support.enums.contains(enumeration.id.as_str()))
            .filter(|enumeration| !common_enum_is_c_style_value(enumeration))
            .map(|enumeration| {
                Self::render_enum_actual_conversions(enumeration, internal_package, contract)
            })
            .for_each(|section| actual_sections.push(section));

        actual_sections.push(Self::render_ffi_exception_actual_conversion(
            internal_package,
        ));

        contract
            .functions
            .iter()
            .filter(|function| function_supported(function, contract, support))
            .map(|function| {
                Self::render_kotlin_jvm_function_actual(function, contract, internal_package)
            })
            .for_each(|section| actual_sections.push(section));

        join_kotlin_sections(actual_sections)
    }

    fn render_common_result_runtime() -> String {
        r#"class FfiException(val code: kotlin.Int, message: kotlin.String) : kotlin.Exception(message)

sealed class BoltFFIResult<out T, out E> {
    data class Ok<T>(val value: T) : BoltFFIResult<T, kotlin.Nothing>()
    data class Err<E>(val error: E) : BoltFFIResult<kotlin.Nothing, E>()

    val isSuccess: kotlin.Boolean get() = this is Ok
    val isFailure: kotlin.Boolean get() = this is Err

    fun getOrThrow(): T = when (this) {
        is Ok -> value
        is Err -> throw when (error) {
            is kotlin.Throwable -> error
            else -> FfiException(-1, error.toString())
        }
    }

    fun getOrNull(): T? = when (this) {
        is Ok -> value
        is Err -> null
    }

    fun exceptionOrNull(): kotlin.Throwable? = when (this) {
        is Ok -> null
        is Err -> when (error) {
            is kotlin.Throwable -> error
            else -> FfiException(-1, error.toString())
        }
    }

    inline fun <R> fold(onSuccess: (T) -> R, onFailure: (E) -> R): R = when (this) {
        is Ok -> onSuccess(value)
        is Err -> onFailure(error)
    }
}"#
        .to_string()
    }

    fn render_custom_type(custom: &ir::definitions::CustomTypeDef) -> String {
        format!(
            "typealias {} = {}",
            NamingConvention::class_name(custom.id.as_str()),
            common_type_name(&custom.repr)
        )
    }

    fn render_common_record(
        record: &ir::definitions::RecordDef,
        contract: &ir::FfiContract,
    ) -> String {
        if record.fields.is_empty() {
            if record.is_error {
                return format!(
                    "{}object {} : kotlin.Exception(\"\")",
                    kdoc_block(&record.doc),
                    NamingConvention::class_name(record.id.as_str())
                );
            }
            return format!(
                "{}object {}",
                kdoc_block(&record.doc),
                NamingConvention::class_name(record.id.as_str())
            );
        }

        let message_field_name = compatible_record_message_field(record, contract)
            .map(|field| NamingConvention::property_name(field.name.as_str()));
        let can_extend_exception = !error_record_has_incompatible_message_field(record, contract);
        let params = record
            .fields
            .iter()
            .map(|field| {
                let name = NamingConvention::property_name(field.name.as_str());
                let prefix = if record.is_error
                    && can_extend_exception
                    && message_field_name.as_deref() == Some(name.as_str())
                {
                    "override "
                } else {
                    ""
                };
                format!(
                    "{prefix}val {}: {}",
                    name,
                    common_type_name(&field.type_expr)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        let class_name = NamingConvention::class_name(record.id.as_str());
        let error_suffix = if record.is_error && can_extend_exception {
            let message_field = message_field_name.unwrap_or_else(|| "\"\"".to_string());
            format!(" : kotlin.Exception({message_field})")
        } else {
            String::new()
        };

        format!(
            "{}data class {class_name}({params}){error_suffix}",
            kdoc_block(&record.doc)
        )
    }

    fn render_record_actual_conversions(
        record: &ir::definitions::RecordDef,
        internal_package: &str,
        contract: &ir::FfiContract,
    ) -> String {
        let class_name = NamingConvention::class_name(record.id.as_str());
        if record.fields.is_empty() {
            return format!(
                "private fun {class_name}.toBoltFfiJvm(): {internal_package}.{class_name} = {internal_package}.{class_name}\n\nprivate fun {internal_package}.{class_name}.toBoltFfiCommon(): {class_name} = {class_name}"
            );
        }

        let to_jvm_args = record
            .fields
            .iter()
            .map(|field| {
                let name = NamingConvention::property_name(field.name.as_str());
                format!(
                    "{} = {}",
                    name,
                    to_jvm_expr(&field.type_expr, &name, contract, internal_package)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let to_common_args = record
            .fields
            .iter()
            .map(|field| {
                let name = NamingConvention::property_name(field.name.as_str());
                format!(
                    "{} = {}",
                    name,
                    to_common_expr(&field.type_expr, &name, contract, internal_package)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "private fun {class_name}.toBoltFfiJvm(): {internal_package}.{class_name} = {internal_package}.{class_name}({to_jvm_args})\n\nprivate fun {internal_package}.{class_name}.toBoltFfiCommon(): {class_name} = {class_name}({to_common_args})"
        )
    }

    fn render_enum_actual_conversions(
        enumeration: &ir::definitions::EnumDef,
        internal_package: &str,
        contract: &ir::FfiContract,
    ) -> String {
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let to_jvm_arms = enum_variants(enumeration)
            .iter()
            .map(|variant| {
                let variant_name = NamingConvention::class_name(variant.name.as_str());
                let fields = enum_variant_fields(&variant.payload);
                if fields.is_empty() {
                    return format!(
                        "    is {class_name}.{variant_name} -> {internal_package}.{class_name}.{variant_name}"
                    );
                }
                let args = fields
                    .iter()
                    .map(|field| {
                        let name = field.kotlin_name();
                        to_jvm_expr(&field.type_expr, &name, contract, internal_package)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "    is {class_name}.{variant_name} -> {internal_package}.{class_name}.{variant_name}({args})"
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let to_common_arms = enum_variants(enumeration)
            .iter()
            .map(|variant| {
                let variant_name = NamingConvention::class_name(variant.name.as_str());
                let fields = enum_variant_fields(&variant.payload);
                if fields.is_empty() {
                    return format!(
                        "    is {internal_package}.{class_name}.{variant_name} -> {class_name}.{variant_name}"
                    );
                }
                let args = fields
                    .iter()
                    .map(|field| {
                        let name = field.kotlin_name();
                        to_common_expr(&field.type_expr, &name, contract, internal_package)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "    is {internal_package}.{class_name}.{variant_name} -> {class_name}.{variant_name}({args})"
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "private fun {class_name}.toBoltFfiJvm(): {internal_package}.{class_name} = when (this) {{\n{to_jvm_arms}\n}}\n\nprivate fun {internal_package}.{class_name}.toBoltFfiCommon(): {class_name} = when (this) {{\n{to_common_arms}\n}}"
        )
    }

    fn render_ffi_exception_actual_conversion(internal_package: &str) -> String {
        format!(
            "private fun {internal_package}.FfiException.toBoltFfiCommon(): FfiException = FfiException(code, message ?: \"\")"
        )
    }

    fn render_common_enum(
        enumeration: &ir::definitions::EnumDef,
        package_name: &str,
        contract: &ir::FfiContract,
    ) -> String {
        if !enumeration.is_error
            && let ir::definitions::EnumRepr::CStyle { tag_type, variants } = &enumeration.repr
        {
            return Self::render_common_c_style_enum(enumeration, *tag_type, variants);
        }

        Self::render_common_sealed_enum(enumeration, package_name, contract)
    }

    fn render_common_c_style_enum(
        enumeration: &ir::definitions::EnumDef,
        tag_type: ir::types::PrimitiveType,
        variants: &[ir::definitions::CStyleVariant],
    ) -> String {
        let value_type = enum_value_type(tag_type);
        let entries = variants
            .iter()
            .map(|variant| {
                format!(
                    "{}({})",
                    NamingConvention::enum_entry_name(variant.name.as_str()),
                    enum_literal(variant.discriminant, tag_type)
                )
            })
            .collect::<Vec<_>>()
            .join(",\n    ");
        let class_name = NamingConvention::class_name(enumeration.id.as_str());

        format!(
            "{}enum class {class_name}(val value: {value_type}) {{\n    {entries};\n\n    companion object {{\n        fun fromValue(value: {value_type}): {class_name} = entries.firstOrNull {{ it.value == value }} ?: throw kotlin.IllegalArgumentException(\"Unknown {class_name} value: $value\")\n    }}\n}}",
            kdoc_block(&enumeration.doc)
        )
    }

    fn render_common_sealed_enum(
        enumeration: &ir::definitions::EnumDef,
        package_name: &str,
        contract: &ir::FfiContract,
    ) -> String {
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let can_extend_exception =
            !error_enum_has_incompatible_message_field(enumeration, contract);
        let error_suffix = if enumeration.is_error && can_extend_exception {
            " : kotlin.Exception()"
        } else {
            ""
        };
        let variant_names = enum_variants(enumeration)
            .iter()
            .map(|variant| NamingConvention::class_name(variant.name.as_str()))
            .collect::<HashSet<_>>();
        let variants = enum_variants(enumeration)
            .iter()
            .map(|variant| {
                let variant_name = NamingConvention::class_name(variant.name.as_str());
                let fields = enum_variant_fields(&variant.payload);
                let doc = kdoc_block(&variant.doc);
                if fields.is_empty() {
                    format!("{doc}    data object {variant_name} : {class_name}()")
                } else {
                    let params = fields
                        .iter()
                        .map(|field| {
                            let name = field.kotlin_name();
                            let prefix = if enumeration.is_error
                                && can_extend_exception
                                && name == "message"
                                && is_throwable_message_type(&field.type_expr, contract)
                            {
                                "override "
                            } else {
                                ""
                            };
                            format!(
                                "{prefix}val {}: {}",
                                name,
                                common_type_name_with_disambiguation(
                                    &field.type_expr,
                                    &variant_names,
                                    package_name
                                )
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{doc}    data class {variant_name}({params}) : {class_name}()")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(
            "{}sealed class {class_name}{error_suffix} {{\n{variants}\n}}",
            kdoc_block(&enumeration.doc)
        )
    }

    fn render_common_function(function: &ir::definitions::FunctionDef) -> String {
        let function_name = NamingConvention::method_name(function.id.as_str());
        let suspend_prefix = if function.is_async() { "suspend " } else { "" };
        let params = Self::render_common_function_params(function);
        let return_type = return_type_name(&function.returns);
        let return_suffix = return_type
            .as_ref()
            .map(|ty| format!(": {ty}"))
            .unwrap_or_default();

        format!(
            "{}expect {suspend_prefix}fun {function_name}({params}){return_suffix}",
            kdoc_block(&function.doc)
        )
    }

    fn render_kotlin_jvm_function_actual(
        function: &ir::definitions::FunctionDef,
        contract: &ir::FfiContract,
        internal_package: &str,
    ) -> String {
        let function_name = NamingConvention::method_name(function.id.as_str());
        let suspend_prefix = if function.is_async() { "suspend " } else { "" };
        let params = Self::render_common_function_params(function);
        let return_type = return_type_name(&function.returns);
        let return_suffix = return_type
            .as_ref()
            .map(|ty| format!(": {ty}"))
            .unwrap_or_default();

        let args = function
            .params
            .iter()
            .map(|param| {
                let name = NamingConvention::param_name(param.name.as_str());
                to_jvm_expr(&param.type_expr, &name, contract, internal_package)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let delegated = format!("{internal_package}.{function_name}({args})");
        let actual_body =
            return_type_expr(&function.returns, delegated, contract, internal_package);
        let return_line = actual_return_line(&function.returns, &actual_body);

        let catch_blocks =
            Self::render_actual_catches(&function.returns, contract, internal_package);
        format!(
            "{}actual {suspend_prefix}fun {function_name}({params}){return_suffix} {{\n    try {{\n{return_line}\n    }}{catch_blocks}\n}}",
            kdoc_block(&function.doc)
        )
    }

    fn render_actual_catches(
        returns: &ir::definitions::ReturnDef,
        contract: &ir::FfiContract,
        internal_package: &str,
    ) -> String {
        let typed_catch = match returns {
            ir::definitions::ReturnDef::Result { err, .. } => {
                typed_error_class_name(err, contract).map(|class_name| {
                    format!(
                        " catch (err: {internal_package}.{class_name}) {{\n        throw err.toBoltFfiCommon()\n    }}"
                    )
                })
            }
            _ => None,
        }
        .unwrap_or_default();

        format!(
            "{typed_catch} catch (err: {internal_package}.FfiException) {{\n        throw err.toBoltFfiCommon()\n    }}"
        )
    }

    fn render_common_function_params(function: &ir::definitions::FunctionDef) -> String {
        function
            .params
            .iter()
            .map(|param| {
                format!(
                    "{}: {}",
                    NamingConvention::param_name(param.name.as_str()),
                    common_type_name(&param.type_expr)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render_build_gradle(package_name: &str, min_sdk: u32) -> String {
        format!(
            r#"import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {{
    kotlin("multiplatform") version "2.3.21"
    id("com.android.library") version "8.5.2"
}}

kotlin {{
    jvm {{
        compilerOptions {{
            jvmTarget.set(JvmTarget.JVM_1_8)
        }}
    }}

    androidTarget {{
        compilerOptions {{
            jvmTarget.set(JvmTarget.JVM_1_8)
        }}
    }}

    sourceSets {{
        commonMain.dependencies {{
            implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")
        }}
    }}
}}

android {{
    namespace = "{package_name}"
    compileSdk = 35

    defaultConfig {{
        minSdk = {min_sdk}
    }}

    compileOptions {{
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }}
}}
"#
        )
    }

    fn render_settings_gradle(module_name: &str) -> String {
        format!(
            r#"pluginManagement {{
    repositories {{
        google()
        mavenCentral()
        gradlePluginPortal()
    }}
}}

dependencyResolutionManagement {{
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {{
        google()
        mavenCentral()
    }}
}}

rootProject.name = "{}-kmp"
"#,
            module_name.to_lowercase()
        )
    }
}

fn join_kotlin_sections(sections: Vec<String>) -> String {
    let mut out = sections
        .into_iter()
        .map(|section| section.trim().to_string())
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    out.push('\n');
    out
}

fn kdoc_block(doc: &Option<String>) -> String {
    doc.as_ref()
        .map(|text| {
            let mut result = "/**\n".to_string();
            text.lines()
                .for_each(|line| result.push_str(&format!(" * {line}\n")));
            result.push_str(" */\n");
            result
        })
        .unwrap_or_default()
}

fn filter_contract_for_kmp_surface(
    contract: &ir::FfiContract,
    support: &KmpSurfaceSupport,
) -> ir::FfiContract {
    let mut catalog = ir::TypeCatalog::new();

    contract
        .catalog
        .all_custom_types()
        .filter(|custom| support.custom_types.contains(custom.id.as_str()))
        .cloned()
        .for_each(|custom| catalog.insert_custom(custom));

    contract
        .catalog
        .all_records()
        .filter(|record| support.records.contains(record.id.as_str()))
        .map(|record| {
            let mut record = record.clone();
            record.constructors.clear();
            record.methods.clear();
            record
        })
        .for_each(|record| catalog.insert_record(record));

    contract
        .catalog
        .all_enums()
        .filter(|enumeration| support.enums.contains(enumeration.id.as_str()))
        .map(|enumeration| {
            let mut enumeration = enumeration.clone();
            enumeration.constructors.clear();
            enumeration.methods.clear();
            enumeration
        })
        .for_each(|enumeration| catalog.insert_enum(enumeration));

    let functions = contract
        .functions
        .iter()
        .filter(|function| function_supported(function, contract, support))
        .cloned()
        .collect();

    ir::FfiContract {
        package: contract.package.clone(),
        catalog,
        functions,
    }
}

fn filter_abi_for_kmp_surface(
    contract: &ir::FfiContract,
    abi: &AbiContract,
    support: &KmpSurfaceSupport,
) -> AbiContract {
    let supported_function_ids = contract
        .functions
        .iter()
        .filter(|function| function_supported(function, contract, support))
        .map(|function| function.id.as_str().to_string())
        .collect::<HashSet<_>>();

    AbiContract {
        package: abi.package.clone(),
        calls: abi
            .calls
            .iter()
            .filter(|call| match &call.id {
                ir::abi::CallId::Function(id) => supported_function_ids.contains(id.as_str()),
                ir::abi::CallId::Method { .. }
                | ir::abi::CallId::Constructor { .. }
                | ir::abi::CallId::RecordMethod { .. }
                | ir::abi::CallId::RecordConstructor { .. }
                | ir::abi::CallId::EnumMethod { .. }
                | ir::abi::CallId::EnumConstructor { .. } => false,
            })
            .cloned()
            .collect(),
        callbacks: Vec::new(),
        streams: Vec::new(),
        records: abi
            .records
            .iter()
            .filter(|record| support.records.contains(record.id.as_str()))
            .cloned()
            .collect(),
        enums: abi
            .enums
            .iter()
            .filter(|enumeration| support.enums.contains(enumeration.id.as_str()))
            .cloned()
            .collect(),
        free_buf: abi.free_buf.clone(),
        atomic_cas: abi.atomic_cas.clone(),
    }
}

fn common_enum_is_c_style_value(enumeration: &ir::definitions::EnumDef) -> bool {
    !enumeration.is_error && matches!(enumeration.repr, EnumRepr::CStyle { .. })
}

fn enum_variants(enumeration: &ir::definitions::EnumDef) -> Vec<KmpEnumVariant> {
    match &enumeration.repr {
        EnumRepr::CStyle { variants, .. } => variants
            .iter()
            .map(|variant| KmpEnumVariant {
                name: variant.name.as_str().to_string(),
                payload: VariantPayload::Unit,
                doc: variant.doc.clone(),
            })
            .collect(),
        EnumRepr::Data { variants, .. } => variants
            .iter()
            .map(|variant| KmpEnumVariant {
                name: variant.name.as_str().to_string(),
                payload: variant.payload.clone(),
                doc: variant.doc.clone(),
            })
            .collect(),
    }
}

fn enum_variant_fields(payload: &VariantPayload) -> Vec<KmpEnumField> {
    match payload {
        VariantPayload::Unit => Vec::new(),
        VariantPayload::Tuple(types) => types
            .iter()
            .enumerate()
            .map(|(index, type_expr)| KmpEnumField {
                name: NamingConvention::property_name(&format!("value{index}")),
                type_expr: type_expr.clone(),
            })
            .collect(),
        VariantPayload::Struct(fields) => fields
            .iter()
            .map(|field| KmpEnumField {
                name: NamingConvention::property_name(field.name.as_str()),
                type_expr: field.type_expr.clone(),
            })
            .collect(),
    }
}

fn has_reserved_common_runtime_type_name(id: &str) -> bool {
    let class_name = NamingConvention::class_name(id);
    KMP_COMMON_RUNTIME_TYPE_NAMES.contains(&class_name.as_str())
}

fn custom_type_supported_with_sets(
    custom: &ir::definitions::CustomTypeDef,
    contract: &ir::FfiContract,
    records: &HashSet<String>,
    enums: &HashSet<String>,
    custom_types: &HashSet<String>,
) -> bool {
    !has_reserved_common_runtime_type_name(custom.id.as_str())
        && type_supported_with_sets(&custom.repr, contract, records, enums, custom_types)
}

fn record_supported_with_sets(
    record: &ir::definitions::RecordDef,
    contract: &ir::FfiContract,
    records: &HashSet<String>,
    enums: &HashSet<String>,
    custom_types: &HashSet<String>,
) -> bool {
    !has_reserved_common_runtime_type_name(record.id.as_str())
        && !error_record_has_incompatible_message_field(record, contract)
        && record.fields.iter().all(|field| {
            type_supported_with_sets(&field.type_expr, contract, records, enums, custom_types)
        })
}

fn compatible_record_message_field<'a>(
    record: &'a ir::definitions::RecordDef,
    contract: &ir::FfiContract,
) -> Option<&'a ir::definitions::FieldDef> {
    record.fields.iter().find(|field| {
        NamingConvention::property_name(field.name.as_str()) == "message"
            && is_throwable_message_type(&field.type_expr, contract)
    })
}

fn error_record_has_incompatible_message_field(
    record: &ir::definitions::RecordDef,
    contract: &ir::FfiContract,
) -> bool {
    record.is_error
        && record.fields.iter().any(|field| {
            NamingConvention::property_name(field.name.as_str()) == "message"
                && !is_throwable_message_type(&field.type_expr, contract)
        })
}

fn error_enum_has_incompatible_message_field(
    enumeration: &ir::definitions::EnumDef,
    contract: &ir::FfiContract,
) -> bool {
    enumeration.is_error
        && enum_variants(enumeration).iter().any(|variant| {
            enum_variant_fields(&variant.payload).iter().any(|field| {
                field.kotlin_name() == "message"
                    && !is_throwable_message_type(&field.type_expr, contract)
            })
        })
}

fn is_throwable_message_type(ty: &ir::types::TypeExpr, contract: &ir::FfiContract) -> bool {
    is_throwable_message_type_inner(ty, contract, &mut HashSet::new())
}

fn is_throwable_message_type_inner(
    ty: &ir::types::TypeExpr,
    contract: &ir::FfiContract,
    visited_custom_types: &mut HashSet<String>,
) -> bool {
    match ty {
        ir::types::TypeExpr::String => true,
        ir::types::TypeExpr::Option(inner) => {
            is_throwable_message_option_inner_type(inner, contract, visited_custom_types)
        }
        ir::types::TypeExpr::Custom(id) => {
            if !visited_custom_types.insert(id.as_str().to_string()) {
                return false;
            }
            contract.catalog.resolve_custom(id).is_some_and(|custom| {
                is_throwable_message_alias_expansion(&custom.repr, contract, visited_custom_types)
            })
        }
        _ => false,
    }
}

fn is_throwable_message_option_inner_type(
    ty: &ir::types::TypeExpr,
    contract: &ir::FfiContract,
    visited_custom_types: &mut HashSet<String>,
) -> bool {
    match ty {
        ir::types::TypeExpr::String => true,
        ir::types::TypeExpr::Custom(id) => {
            if !visited_custom_types.insert(id.as_str().to_string()) {
                return false;
            }
            contract.catalog.resolve_custom(id).is_some_and(|custom| {
                is_throwable_message_alias_expansion(&custom.repr, contract, visited_custom_types)
            })
        }
        _ => false,
    }
}

fn is_throwable_message_alias_expansion(
    ty: &ir::types::TypeExpr,
    contract: &ir::FfiContract,
    visited_custom_types: &mut HashSet<String>,
) -> bool {
    match ty {
        ir::types::TypeExpr::String => true,
        ir::types::TypeExpr::Option(inner) => {
            is_throwable_message_option_inner_type(inner, contract, visited_custom_types)
        }
        ir::types::TypeExpr::Custom(id) => {
            if !visited_custom_types.insert(id.as_str().to_string()) {
                return false;
            }
            contract.catalog.resolve_custom(id).is_some_and(|custom| {
                is_throwable_message_alias_expansion(&custom.repr, contract, visited_custom_types)
            })
        }
        _ => false,
    }
}

fn typed_error_class_name(ty: &ir::types::TypeExpr, contract: &ir::FfiContract) -> Option<String> {
    fn resolve(
        ty: &ir::types::TypeExpr,
        contract: &ir::FfiContract,
        visited_custom_types: &mut HashSet<String>,
    ) -> Option<String> {
        match ty {
            ir::types::TypeExpr::Enum(id) => contract
                .catalog
                .resolve_enum(id)
                .filter(|enumeration| enumeration.is_error)
                .map(|_| NamingConvention::class_name(id.as_str())),
            ir::types::TypeExpr::Record(id) => contract
                .catalog
                .resolve_record(id)
                .filter(|record| record.is_error)
                .map(|_| NamingConvention::class_name(id.as_str())),
            ir::types::TypeExpr::Custom(id) => {
                if !visited_custom_types.insert(id.as_str().to_string()) {
                    return None;
                }
                contract
                    .catalog
                    .resolve_custom(id)
                    .and_then(|custom| resolve(&custom.repr, contract, visited_custom_types))
            }
            ir::types::TypeExpr::Option(inner) => resolve(inner, contract, visited_custom_types),
            _ => None,
        }
    }

    resolve(ty, contract, &mut HashSet::new())
}

fn type_supported(
    ty: &ir::types::TypeExpr,
    contract: &ir::FfiContract,
    support: &KmpSurfaceSupport,
) -> bool {
    type_supported_with_sets(
        ty,
        contract,
        &support.records,
        &support.enums,
        &support.custom_types,
    )
}

fn type_supported_with_sets(
    ty: &ir::types::TypeExpr,
    contract: &ir::FfiContract,
    records: &HashSet<String>,
    enums: &HashSet<String>,
    custom_types: &HashSet<String>,
) -> bool {
    let _ = contract;
    match ty {
        ir::types::TypeExpr::Void
        | ir::types::TypeExpr::Primitive(_)
        | ir::types::TypeExpr::String => true,
        ir::types::TypeExpr::Vec(inner) | ir::types::TypeExpr::Option(inner) => {
            type_supported_with_sets(inner, contract, records, enums, custom_types)
        }
        ir::types::TypeExpr::Record(id) => records.contains(id.as_str()),
        ir::types::TypeExpr::Enum(id) => enums.contains(id.as_str()),
        ir::types::TypeExpr::Custom(id) => custom_types.contains(id.as_str()),
        ir::types::TypeExpr::Result { ok, err } => {
            type_supported_with_sets(ok, contract, records, enums, custom_types)
                && type_supported_with_sets(err, contract, records, enums, custom_types)
        }
        ir::types::TypeExpr::Builtin(_)
        | ir::types::TypeExpr::Callback(_)
        | ir::types::TypeExpr::Handle(_) => false,
    }
}

fn enum_supported_with_sets(
    enumeration: &ir::definitions::EnumDef,
    contract: &ir::FfiContract,
    records: &HashSet<String>,
    enums: &HashSet<String>,
    custom_types: &HashSet<String>,
) -> bool {
    if has_reserved_common_runtime_type_name(enumeration.id.as_str())
        || error_enum_has_incompatible_message_field(enumeration, contract)
    {
        return false;
    }

    match &enumeration.repr {
        EnumRepr::CStyle { .. } => true,
        EnumRepr::Data { variants, .. } => variants.iter().all(|variant| {
            enum_variant_fields(&variant.payload).iter().all(|field| {
                type_supported_with_sets(&field.type_expr, contract, records, enums, custom_types)
            })
        }),
    }
}

fn return_supported(
    returns: &ir::definitions::ReturnDef,
    contract: &ir::FfiContract,
    support: &KmpSurfaceSupport,
) -> bool {
    match returns {
        ir::definitions::ReturnDef::Void => true,
        ir::definitions::ReturnDef::Value(ty) => type_supported(ty, contract, support),
        ir::definitions::ReturnDef::Result { ok, err } => {
            type_supported(ok, contract, support) && type_supported(err, contract, support)
        }
    }
}

fn function_supported(
    function: &ir::definitions::FunctionDef,
    contract: &ir::FfiContract,
    support: &KmpSurfaceSupport,
) -> bool {
    return_supported(&function.returns, contract, support)
        && function
            .params
            .iter()
            .all(|param| type_supported(&param.type_expr, contract, support))
}

fn common_type_name(ty: &ir::types::TypeExpr) -> String {
    match ty {
        ir::types::TypeExpr::Void => "Unit".to_string(),
        ir::types::TypeExpr::Primitive(primitive) => primitive_type_name(*primitive),
        ir::types::TypeExpr::String => "String".to_string(),
        ir::types::TypeExpr::Vec(inner) => vec_type_name(inner),
        ir::types::TypeExpr::Option(inner) => format!("{}?", common_type_name(inner)),
        ir::types::TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Custom(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Builtin(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Handle(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Callback(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Result { ok, err } => {
            format!(
                "BoltFFIResult<{}, {}>",
                common_type_name(ok),
                common_type_name(err)
            )
        }
    }
}

fn common_type_name_with_disambiguation(
    ty: &ir::types::TypeExpr,
    reserved_names: &HashSet<String>,
    package_name: &str,
) -> String {
    match ty {
        ir::types::TypeExpr::Void => {
            disambiguated_kotlin_type_name("Unit", reserved_names, "kotlin")
        }
        ir::types::TypeExpr::Primitive(primitive) => {
            let name = primitive_type_name(*primitive);
            disambiguated_kotlin_type_name(&name, reserved_names, "kotlin")
        }
        ir::types::TypeExpr::String => {
            disambiguated_kotlin_type_name("String", reserved_names, "kotlin")
        }
        ir::types::TypeExpr::Vec(inner) => match inner.as_ref() {
            ir::types::TypeExpr::Primitive(_) => {
                let name = vec_type_name(inner);
                disambiguated_kotlin_type_name(&name, reserved_names, "kotlin")
            }
            _ => {
                let list_name =
                    disambiguated_kotlin_type_name("List", reserved_names, "kotlin.collections");
                format!(
                    "{list_name}<{}>",
                    common_type_name_with_disambiguation(inner, reserved_names, package_name)
                )
            }
        },
        ir::types::TypeExpr::Option(inner) => format!(
            "{}?",
            common_type_name_with_disambiguation(inner, reserved_names, package_name)
        ),
        ir::types::TypeExpr::Result { ok, err } => format!(
            "{}<{}, {}>",
            disambiguated_kotlin_type_name("BoltFFIResult", reserved_names, package_name),
            common_type_name_with_disambiguation(ok, reserved_names, package_name),
            common_type_name_with_disambiguation(err, reserved_names, package_name)
        ),
        ir::types::TypeExpr::Record(id) => {
            disambiguated_class_name(id.as_str(), reserved_names, package_name)
        }
        ir::types::TypeExpr::Enum(id) => {
            disambiguated_class_name(id.as_str(), reserved_names, package_name)
        }
        ir::types::TypeExpr::Custom(id) => {
            disambiguated_class_name(id.as_str(), reserved_names, package_name)
        }
        ir::types::TypeExpr::Builtin(id) => {
            disambiguated_class_name(id.as_str(), reserved_names, package_name)
        }
        ir::types::TypeExpr::Handle(id) => {
            disambiguated_class_name(id.as_str(), reserved_names, package_name)
        }
        ir::types::TypeExpr::Callback(id) => {
            disambiguated_class_name(id.as_str(), reserved_names, package_name)
        }
    }
}

fn disambiguated_kotlin_type_name(
    name: &str,
    reserved_names: &HashSet<String>,
    package_name: &str,
) -> String {
    if reserved_names.contains(name) {
        format!("{package_name}.{name}")
    } else {
        name.to_string()
    }
}

fn disambiguated_class_name(
    id: &str,
    reserved_names: &HashSet<String>,
    package_name: &str,
) -> String {
    let class_name = NamingConvention::class_name(id);
    if reserved_names.contains(&class_name) {
        format!("{package_name}.{class_name}")
    } else {
        class_name
    }
}

fn vec_type_name(inner: &ir::types::TypeExpr) -> String {
    match inner {
        ir::types::TypeExpr::Primitive(primitive) => match primitive {
            ir::types::PrimitiveType::I32 | ir::types::PrimitiveType::U32 => "IntArray".to_string(),
            ir::types::PrimitiveType::I16 | ir::types::PrimitiveType::U16 => {
                "ShortArray".to_string()
            }
            ir::types::PrimitiveType::I64
            | ir::types::PrimitiveType::U64
            | ir::types::PrimitiveType::ISize
            | ir::types::PrimitiveType::USize => "LongArray".to_string(),
            ir::types::PrimitiveType::F32 => "FloatArray".to_string(),
            ir::types::PrimitiveType::F64 => "DoubleArray".to_string(),
            ir::types::PrimitiveType::U8 | ir::types::PrimitiveType::I8 => "ByteArray".to_string(),
            ir::types::PrimitiveType::Bool => "BooleanArray".to_string(),
        },
        _ => format!("List<{}>", common_type_name(inner)),
    }
}

fn primitive_type_name(primitive: ir::types::PrimitiveType) -> String {
    match primitive {
        ir::types::PrimitiveType::Bool => "Boolean".to_string(),
        ir::types::PrimitiveType::I8 => "Byte".to_string(),
        ir::types::PrimitiveType::U8 => "UByte".to_string(),
        ir::types::PrimitiveType::I16 => "Short".to_string(),
        ir::types::PrimitiveType::U16 => "UShort".to_string(),
        ir::types::PrimitiveType::I32 => "Int".to_string(),
        ir::types::PrimitiveType::U32 => "UInt".to_string(),
        ir::types::PrimitiveType::I64 | ir::types::PrimitiveType::ISize => "Long".to_string(),
        ir::types::PrimitiveType::U64 | ir::types::PrimitiveType::USize => "ULong".to_string(),
        ir::types::PrimitiveType::F32 => "Float".to_string(),
        ir::types::PrimitiveType::F64 => "Double".to_string(),
    }
}

fn enum_value_type(primitive: ir::types::PrimitiveType) -> String {
    match primitive {
        ir::types::PrimitiveType::I8 | ir::types::PrimitiveType::U8 => "Byte".to_string(),
        ir::types::PrimitiveType::I16 | ir::types::PrimitiveType::U16 => "Short".to_string(),
        ir::types::PrimitiveType::I32 | ir::types::PrimitiveType::U32 => "Int".to_string(),
        ir::types::PrimitiveType::I64
        | ir::types::PrimitiveType::U64
        | ir::types::PrimitiveType::ISize
        | ir::types::PrimitiveType::USize => "Long".to_string(),
        _ => primitive_type_name(primitive),
    }
}

fn enum_literal(value: i128, primitive: ir::types::PrimitiveType) -> String {
    kotlin_integer_literal(value, &enum_value_type(primitive))
}

fn kotlin_integer_literal(value: i128, kotlin_type: &str) -> String {
    match kotlin_type {
        "Byte" => format!("({value}L).toByte()"),
        "Short" => format!("({value}L).toShort()"),
        "Int" => {
            if i32::try_from(value).is_ok() {
                value.to_string()
            } else {
                format!("({value}L).toInt()")
            }
        }
        "Long" => {
            if i64::try_from(value).is_ok() {
                format!("{value}L")
            } else {
                format!("({value}uL).toLong()")
            }
        }
        _ => value.to_string(),
    }
}

fn return_type_name(returns: &ir::definitions::ReturnDef) -> Option<String> {
    match returns {
        ir::definitions::ReturnDef::Void => None,
        ir::definitions::ReturnDef::Value(ty) => Some(common_type_name(ty)),
        ir::definitions::ReturnDef::Result { ok, .. } => Some(common_type_name(ok)),
    }
}

fn to_jvm_expr(
    ty: &ir::types::TypeExpr,
    expr: &str,
    contract: &ir::FfiContract,
    internal_package: &str,
) -> String {
    match ty {
        ir::types::TypeExpr::Primitive(_) => expr.to_string(),
        ir::types::TypeExpr::Record(_) => format!("{expr}.toBoltFfiJvm()"),
        ir::types::TypeExpr::Enum(id) => {
            let class_name = NamingConvention::class_name(id.as_str());
            if contract
                .catalog
                .resolve_enum(id)
                .map(common_enum_is_c_style_value)
                .unwrap_or(false)
            {
                format!("{internal_package}.{class_name}.fromValue({expr}.value)")
            } else {
                format!("{expr}.toBoltFfiJvm()")
            }
        }
        ir::types::TypeExpr::Vec(inner) => to_jvm_vec_expr(inner, expr, contract, internal_package),
        ir::types::TypeExpr::Option(inner) => {
            format!(
                "{expr}?.let {{ {} }}",
                to_jvm_expr(inner, "it", contract, internal_package)
            )
        }
        ir::types::TypeExpr::Custom(id) => contract
            .catalog
            .resolve_custom(id)
            .map(|custom| to_jvm_expr(&custom.repr, expr, contract, internal_package))
            .unwrap_or_else(|| expr.to_string()),
        ir::types::TypeExpr::Result { ok, err } => {
            let ok_expr = to_jvm_expr(ok, "boltffiResult.value", contract, internal_package);
            let err_expr = to_jvm_expr(err, "boltffiResult.error", contract, internal_package);
            format!(
                "when (val boltffiResult = {expr}) {{ is BoltFFIResult.Ok -> {internal_package}.BoltFFIResult.Ok({ok_expr}); is BoltFFIResult.Err -> {internal_package}.BoltFFIResult.Err({err_expr}) }}"
            )
        }
        _ => expr.to_string(),
    }
}

fn to_jvm_vec_expr(
    ty: &ir::types::TypeExpr,
    expr: &str,
    contract: &ir::FfiContract,
    internal_package: &str,
) -> String {
    match ty {
        ir::types::TypeExpr::Primitive(_) => expr.to_string(),
        _ => format!(
            "{expr}.map {{ {} }}",
            to_jvm_expr(ty, "it", contract, internal_package)
        ),
    }
}

fn to_common_expr(
    ty: &ir::types::TypeExpr,
    expr: &str,
    contract: &ir::FfiContract,
    internal_package: &str,
) -> String {
    match ty {
        ir::types::TypeExpr::Primitive(_) => expr.to_string(),
        ir::types::TypeExpr::Record(_) => format!("{expr}.toBoltFfiCommon()"),
        ir::types::TypeExpr::Enum(id) => {
            let class_name = NamingConvention::class_name(id.as_str());
            if contract
                .catalog
                .resolve_enum(id)
                .map(common_enum_is_c_style_value)
                .unwrap_or(false)
            {
                format!("{class_name}.fromValue({expr}.value)")
            } else {
                format!("{expr}.toBoltFfiCommon()")
            }
        }
        ir::types::TypeExpr::Vec(inner) => {
            to_common_vec_expr(inner, expr, contract, internal_package)
        }
        ir::types::TypeExpr::Option(inner) => {
            format!(
                "{expr}?.let {{ {} }}",
                to_common_expr(inner, "it", contract, internal_package)
            )
        }
        ir::types::TypeExpr::Custom(id) => contract
            .catalog
            .resolve_custom(id)
            .map(|custom| to_common_expr(&custom.repr, expr, contract, internal_package))
            .unwrap_or_else(|| expr.to_string()),
        ir::types::TypeExpr::Result { ok, err } => {
            let ok_expr = to_common_expr(ok, "boltffiResult.value", contract, internal_package);
            let err_expr = to_common_expr(err, "boltffiResult.error", contract, internal_package);
            format!(
                "when (val boltffiResult = {expr}) {{ is {internal_package}.BoltFFIResult.Ok -> BoltFFIResult.Ok({ok_expr}); is {internal_package}.BoltFFIResult.Err -> BoltFFIResult.Err({err_expr}) }}"
            )
        }
        _ => expr.to_string(),
    }
}

fn to_common_vec_expr(
    ty: &ir::types::TypeExpr,
    expr: &str,
    contract: &ir::FfiContract,
    internal_package: &str,
) -> String {
    match ty {
        ir::types::TypeExpr::Primitive(_) => expr.to_string(),
        _ => format!(
            "{expr}.map {{ {} }}",
            to_common_expr(ty, "it", contract, internal_package)
        ),
    }
}

fn actual_return_line(returns: &ir::definitions::ReturnDef, actual_body: &str) -> String {
    match returns {
        ir::definitions::ReturnDef::Void => format!("        {actual_body}"),
        ir::definitions::ReturnDef::Value(_) | ir::definitions::ReturnDef::Result { .. } => {
            format!("        return {actual_body}")
        }
    }
}

fn return_type_expr(
    returns: &ir::definitions::ReturnDef,
    delegated: String,
    contract: &ir::FfiContract,
    internal_package: &str,
) -> String {
    match returns {
        ir::definitions::ReturnDef::Void => delegated,
        ir::definitions::ReturnDef::Value(ty) => {
            to_common_expr(ty, &delegated, contract, internal_package)
        }
        ir::definitions::ReturnDef::Result { ok, .. } => {
            to_common_expr(ok, &delegated, contract, internal_package)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_record() -> ir::definitions::RecordDef {
        ir::definitions::RecordDef {
            id: "Empty".into(),
            is_repr_c: false,
            is_error: false,
            fields: Vec::new(),
            constructors: Vec::new(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        }
    }

    fn field(name: &str, type_expr: ir::types::TypeExpr) -> ir::definitions::FieldDef {
        ir::definitions::FieldDef {
            name: name.into(),
            type_expr,
            doc: None,
            default: None,
        }
    }

    fn error_record(id: &str) -> ir::definitions::RecordDef {
        ir::definitions::RecordDef {
            id: id.into(),
            is_repr_c: false,
            is_error: true,
            fields: vec![field("message", ir::types::TypeExpr::String)],
            constructors: Vec::new(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        }
    }

    fn custom_type(id: &str, repr: ir::types::TypeExpr) -> ir::definitions::CustomTypeDef {
        ir::definitions::CustomTypeDef {
            id: id.into(),
            rust_type: ir::ids::QualifiedName::new(format!("demo::{id}")),
            repr,
            converters: ir::ids::ConverterPath {
                into_ffi: ir::ids::QualifiedName::new("into_ffi"),
                try_from_ffi: ir::ids::QualifiedName::new("try_from_ffi"),
            },
            doc: None,
        }
    }

    fn empty_contract() -> ir::FfiContract {
        ir::FfiContract {
            package: ir::PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog: ir::TypeCatalog::new(),
            functions: Vec::new(),
        }
    }

    fn sync_function(
        id: &str,
        returns: ir::definitions::ReturnDef,
    ) -> ir::definitions::FunctionDef {
        ir::definitions::FunctionDef {
            id: id.into(),
            params: Vec::new(),
            returns,
            execution_kind: boltffi_ffi_rules::callable::ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn empty_records_render_as_objects() {
        let record = empty_record();

        let common = KMPEmitter::render_common_record(&record, &empty_contract());
        let actual = KMPEmitter::render_record_actual_conversions(
            &record,
            "com.example.demo.jvm",
            &empty_contract(),
        );

        assert_eq!(common, "object Empty");
        assert!(actual.contains("= com.example.demo.jvm.Empty"));
        assert!(actual.contains("= Empty"));
        assert!(!common.contains("data class Empty()"));
        assert!(!actual.contains("Empty()"));
    }

    #[test]
    fn common_surface_reserves_result_runtime_type_names() {
        let mut contract = empty_contract();
        let mut ffi_exception = empty_record();
        ffi_exception.id = "FfiException".into();
        ffi_exception.fields = vec![field(
            "code",
            ir::types::TypeExpr::Primitive(ir::types::PrimitiveType::I32),
        )];
        contract.catalog.insert_record(ffi_exception);
        contract.catalog.insert_custom(custom_type(
            "bolt_f_f_i_result",
            ir::types::TypeExpr::String,
        ));
        contract.functions.push(sync_function(
            "load",
            ir::definitions::ReturnDef::Value(ir::types::TypeExpr::Record("FfiException".into())),
        ));

        let support = KmpSurfaceSupport::for_contract(&contract);
        let common = KMPEmitter::render_common_surface(&contract, "com.example.demo", &support);
        let internal_contract = filter_contract_for_kmp_surface(&contract, &support);

        assert!(!support.records.contains("FfiException"));
        assert!(!support.custom_types.contains("bolt_f_f_i_result"));
        assert_eq!(common.matches("class FfiException").count(), 1);
        assert_eq!(common.matches("sealed class BoltFFIResult").count(), 1);
        assert!(!common.contains("data class FfiException("));
        assert!(!common.contains("typealias BoltFFIResult"));
        assert!(!common.contains("expect fun load"));
        assert!(common.contains("Unsupported in the initial KMP generator slice: load"));
        assert_eq!(internal_contract.catalog.all_records().count(), 0);
        assert_eq!(internal_contract.catalog.all_custom_types().count(), 0);
        assert!(internal_contract.functions.is_empty());
    }

    #[test]
    fn common_runtime_qualifies_stdlib_exception_types() {
        let mut contract = empty_contract();
        let mut exception = empty_record();
        exception.id = "Exception".into();
        let mut throwable = empty_record();
        throwable.id = "Throwable".into();
        let mut illegal_argument_exception = empty_record();
        illegal_argument_exception.id = "IllegalArgumentException".into();
        contract.catalog.insert_record(exception);
        contract.catalog.insert_record(throwable);
        contract.catalog.insert_record(illegal_argument_exception);
        for id in ["String", "Int", "Boolean", "Nothing"] {
            let mut record = empty_record();
            record.id = id.into();
            contract.catalog.insert_record(record);
        }
        contract.catalog.insert_enum(ir::definitions::EnumDef {
            id: "Status".into(),
            repr: EnumRepr::CStyle {
                tag_type: ir::types::PrimitiveType::I32,
                variants: vec![ir::definitions::CStyleVariant {
                    name: "Ready".into(),
                    discriminant: 0,
                    doc: None,
                }],
            },
            is_error: false,
            constructors: Vec::new(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        });

        let support = KmpSurfaceSupport::for_contract(&contract);
        let common = KMPEmitter::render_common_surface(&contract, "com.example.demo", &support);

        assert!(support.records.contains("Exception"));
        assert!(support.records.contains("Throwable"));
        assert!(support.records.contains("IllegalArgumentException"));
        assert!(support.records.contains("String"));
        assert!(support.records.contains("Int"));
        assert!(support.records.contains("Boolean"));
        assert!(support.records.contains("Nothing"));
        assert!(common.contains("object Exception"));
        assert!(common.contains("object Throwable"));
        assert!(common.contains("object IllegalArgumentException"));
        assert!(common.contains("object String"));
        assert!(common.contains("object Int"));
        assert!(common.contains("object Boolean"));
        assert!(common.contains("object Nothing"));
        assert!(common.contains(
            "class FfiException(val code: kotlin.Int, message: kotlin.String) : kotlin.Exception(message)"
        ));
        assert!(common.contains("BoltFFIResult<T, kotlin.Nothing>"));
        assert!(common.contains("BoltFFIResult<kotlin.Nothing, E>"));
        assert!(common.contains("val isSuccess: kotlin.Boolean"));
        assert!(common.contains("is kotlin.Throwable -> error"));
        assert!(common.contains("fun exceptionOrNull(): kotlin.Throwable?"));
        assert!(common.contains("throw kotlin.IllegalArgumentException"));
    }

    #[test]
    fn error_record_message_fields_use_normalized_kotlin_name() {
        let mut record = error_record("ServiceError");
        record.fields = vec![field("Message", ir::types::TypeExpr::String)];

        let common = KMPEmitter::render_common_record(&record, &empty_contract());

        assert_eq!(
            common,
            "data class ServiceError(override val message: String) : kotlin.Exception(message)"
        );
    }

    #[test]
    fn error_record_nullable_message_fields_override_throwable_message() {
        let mut record = error_record("ServiceError");
        record.fields = vec![field(
            "message",
            ir::types::TypeExpr::Option(Box::new(ir::types::TypeExpr::String)),
        )];

        let common = KMPEmitter::render_common_record(&record, &empty_contract());

        assert_eq!(
            common,
            "data class ServiceError(override val message: String?) : kotlin.Exception(message)"
        );
    }

    #[test]
    fn error_record_message_fields_use_string_custom_aliases() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_custom(custom_type("MessageText", ir::types::TypeExpr::String));
        let mut record = error_record("ServiceError");
        record.fields = vec![field(
            "message",
            ir::types::TypeExpr::Custom("MessageText".into()),
        )];
        contract.catalog.insert_record(record.clone());

        let support = KmpSurfaceSupport::for_contract(&contract);
        let common = KMPEmitter::render_common_record(&record, &contract);

        assert!(support.custom_types.contains("MessageText"));
        assert!(support.records.contains("ServiceError"));
        assert_eq!(
            common,
            "data class ServiceError(override val message: MessageText) : kotlin.Exception(message)"
        );
    }

    #[test]
    fn error_record_option_message_fields_accept_nullable_string_aliases() {
        let mut contract = empty_contract();
        contract.catalog.insert_custom(custom_type(
            "MessageText",
            ir::types::TypeExpr::Option(Box::new(ir::types::TypeExpr::String)),
        ));
        let mut record = error_record("ServiceError");
        record.fields = vec![field(
            "message",
            ir::types::TypeExpr::Option(Box::new(ir::types::TypeExpr::Custom(
                "MessageText".into(),
            ))),
        )];
        contract.catalog.insert_record(record.clone());
        contract.functions.push(sync_function(
            "load",
            ir::definitions::ReturnDef::Result {
                ok: ir::types::TypeExpr::String,
                err: ir::types::TypeExpr::Record("ServiceError".into()),
            },
        ));

        let support = KmpSurfaceSupport::for_contract(&contract);
        let common = KMPEmitter::render_common_surface(&contract, "com.example.demo", &support);

        assert!(support.custom_types.contains("MessageText"));
        assert!(support.records.contains("ServiceError"));
        assert!(function_supported(
            &contract.functions[0],
            &contract,
            &support
        ));
        assert!(common.contains("typealias MessageText = String?"));
        assert!(common.contains(
            "data class ServiceError(override val message: MessageText?) : kotlin.Exception(message)"
        ));
        assert!(common.contains("expect fun load(): String"));
    }

    #[test]
    fn error_record_non_string_message_field_is_not_supported() {
        let mut contract = empty_contract();
        let mut record = error_record("ServiceError");
        record.fields = vec![field(
            "message",
            ir::types::TypeExpr::Primitive(ir::types::PrimitiveType::I32),
        )];
        contract.catalog.insert_record(record.clone());
        contract.functions.push(sync_function(
            "load",
            ir::definitions::ReturnDef::Result {
                ok: ir::types::TypeExpr::String,
                err: ir::types::TypeExpr::Record("ServiceError".into()),
            },
        ));

        let support = KmpSurfaceSupport::for_contract(&contract);
        let common = KMPEmitter::render_common_surface(&contract, "com.example.demo", &support);
        let direct = KMPEmitter::render_common_record(&record, &contract);

        assert!(!support.records.contains("ServiceError"));
        assert!(!direct.contains("override val message: Int"));
        assert_eq!(direct, "data class ServiceError(val message: Int)");
        assert!(!common.contains("data class ServiceError("));
        assert!(!common.contains("expect fun load"));
        assert!(common.contains("Unsupported in the initial KMP generator slice: load"));
    }

    #[test]
    fn emit_filters_unsupported_kmp_surface_from_internal_sources() {
        let mut contract = empty_contract();
        let mut record = error_record("ServiceError");
        record.fields = vec![field(
            "message",
            ir::types::TypeExpr::Primitive(ir::types::PrimitiveType::I32),
        )];
        contract.catalog.insert_record(record);
        contract.functions.push(sync_function(
            "load",
            ir::definitions::ReturnDef::Result {
                ok: ir::types::TypeExpr::String,
                err: ir::types::TypeExpr::Record("ServiceError".into()),
            },
        ));
        let abi = ir::Lowerer::new(&contract).to_abi_contract();

        let output = KMPEmitter::emit(
            &contract,
            &abi,
            KMPOptions {
                package_name: "com.example.demo".to_string(),
                module_name: "Demo".to_string(),
                min_sdk: 23,
                kotlin_options: KotlinOptions::default(),
            },
        );
        let common = output
            .files
            .iter()
            .find(|file| {
                file.relative_path.as_path()
                    == Path::new("src/commonMain/kotlin/com/example/demo/Demo.kt")
            })
            .expect("common source should be emitted");
        let internal_jvm = output
            .files
            .iter()
            .find(|file| {
                file.relative_path.as_path()
                    == Path::new("src/jvmMain/kotlin/com/example/demo/jvm/Demo.kt")
            })
            .expect("jvm internal source should be emitted");
        let jni_glue = output
            .files
            .iter()
            .find(|file| file.relative_path.as_path() == Path::new("src/jvmMain/c/jni_glue.c"))
            .expect("jvm JNI glue should be emitted");

        assert!(!common.contents.contains("data class ServiceError("));
        assert!(!common.contents.contains("expect fun load"));
        assert!(
            common
                .contents
                .contains("Unsupported in the initial KMP generator slice: load")
        );
        assert!(!internal_jvm.contents.contains("data class ServiceError("));
        assert!(!internal_jvm.contents.contains("fun load("));
        assert!(!jni_glue.contents.contains("boltffi_load"));
    }

    #[test]
    fn error_enum_message_fields_override_throwable_message() {
        let enumeration = ir::definitions::EnumDef {
            id: "DomainError".into(),
            repr: EnumRepr::Data {
                tag_type: ir::types::PrimitiveType::I32,
                variants: vec![ir::definitions::DataVariant {
                    name: "Invalid".into(),
                    discriminant: 0,
                    payload: VariantPayload::Struct(vec![field(
                        "message",
                        ir::types::TypeExpr::String,
                    )]),
                    doc: None,
                }],
            },
            is_error: true,
            constructors: Vec::new(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        };

        let common = KMPEmitter::render_common_sealed_enum(
            &enumeration,
            "com.example.demo",
            &empty_contract(),
        );

        assert!(
            common.contains("data class Invalid(override val message: String) : DomainError()")
        );
        assert!(!common.contains("data class Invalid(val message: String)"));
    }

    #[test]
    fn emit_makes_error_enum_message_fields_override_throwable_message() {
        let mut contract = empty_contract();
        let enumeration = ir::definitions::EnumDef {
            id: "DomainError".into(),
            repr: EnumRepr::Data {
                tag_type: ir::types::PrimitiveType::I32,
                variants: vec![ir::definitions::DataVariant {
                    name: "Invalid".into(),
                    discriminant: 0,
                    payload: VariantPayload::Struct(vec![field(
                        "message",
                        ir::types::TypeExpr::String,
                    )]),
                    doc: None,
                }],
            },
            is_error: true,
            constructors: Vec::new(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        };
        contract.catalog.insert_enum(enumeration);
        contract.functions.push(sync_function(
            "load",
            ir::definitions::ReturnDef::Result {
                ok: ir::types::TypeExpr::String,
                err: ir::types::TypeExpr::Enum("DomainError".into()),
            },
        ));
        let abi = ir::Lowerer::new(&contract).to_abi_contract();

        let support = KmpSurfaceSupport::for_contract(&contract);
        assert!(support.enums.contains("DomainError"));
        assert!(function_supported(
            &contract.functions[0],
            &contract,
            &support
        ));

        let output = KMPEmitter::emit(
            &contract,
            &abi,
            KMPOptions {
                package_name: "com.example.demo".to_string(),
                module_name: "Demo".to_string(),
                min_sdk: 23,
                kotlin_options: KotlinOptions::default(),
            },
        );
        let common = output
            .files
            .iter()
            .find(|file| {
                file.relative_path.as_path()
                    == Path::new("src/commonMain/kotlin/com/example/demo/Demo.kt")
            })
            .expect("common source should be emitted");
        let internal_jvm = output
            .files
            .iter()
            .find(|file| {
                file.relative_path.as_path()
                    == Path::new("src/jvmMain/kotlin/com/example/demo/jvm/Demo.kt")
            })
            .expect("jvm internal source should be emitted");

        assert!(
            common
                .contents
                .contains("data class Invalid(override val message: String) : DomainError()")
        );
        assert!(
            internal_jvm
                .contents
                .contains("data class Invalid(override val message: String) : DomainError()")
        );
    }

    #[test]
    fn error_enum_non_string_message_field_is_not_supported() {
        let mut contract = empty_contract();
        let enumeration = ir::definitions::EnumDef {
            id: "DomainError".into(),
            repr: EnumRepr::Data {
                tag_type: ir::types::PrimitiveType::I32,
                variants: vec![ir::definitions::DataVariant {
                    name: "Invalid".into(),
                    discriminant: 0,
                    payload: VariantPayload::Struct(vec![field(
                        "message",
                        ir::types::TypeExpr::Primitive(ir::types::PrimitiveType::I32),
                    )]),
                    doc: None,
                }],
            },
            is_error: true,
            constructors: Vec::new(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        };
        contract.catalog.insert_enum(enumeration.clone());
        contract.functions.push(sync_function(
            "load",
            ir::definitions::ReturnDef::Result {
                ok: ir::types::TypeExpr::String,
                err: ir::types::TypeExpr::Enum("DomainError".into()),
            },
        ));

        let support = KmpSurfaceSupport::for_contract(&contract);
        let common = KMPEmitter::render_common_surface(&contract, "com.example.demo", &support);
        let direct =
            KMPEmitter::render_common_sealed_enum(&enumeration, "com.example.demo", &contract);

        assert!(!support.enums.contains("DomainError"));
        assert!(!direct.contains("sealed class DomainError : kotlin.Exception()"));
        assert!(!direct.contains("override val message: Int"));
        assert!(!common.contains("sealed class DomainError"));
        assert!(!common.contains("expect fun load"));
        assert!(common.contains("Unsupported in the initial KMP generator slice: load"));
    }

    #[test]
    fn result_actual_catches_errors_behind_custom_aliases() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(error_record("ServiceError"));
        contract.catalog.insert_custom(custom_type(
            "ServiceFailureWire",
            ir::types::TypeExpr::Record("ServiceError".into()),
        ));
        contract.catalog.insert_custom(custom_type(
            "ServiceFailure",
            ir::types::TypeExpr::Custom("ServiceFailureWire".into()),
        ));
        contract.functions.push(ir::definitions::FunctionDef {
            id: "load".into(),
            params: Vec::new(),
            returns: ir::definitions::ReturnDef::Result {
                ok: ir::types::TypeExpr::String,
                err: ir::types::TypeExpr::Custom("ServiceFailure".into()),
            },
            execution_kind: boltffi_ffi_rules::callable::ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let support = KmpSurfaceSupport::for_contract(&contract);
        assert!(function_supported(
            &contract.functions[0],
            &contract,
            &support
        ));

        let actual = KMPEmitter::render_kotlin_jvm_function_actual(
            &contract.functions[0],
            &contract,
            "com.example.demo.jvm",
        );

        assert!(actual.contains("catch (err: com.example.demo.jvm.ServiceError)"));
        assert!(!actual.contains("catch (err: com.example.demo.jvm.ServiceFailure)"));
        assert!(actual.contains("catch (err: com.example.demo.jvm.FfiException)"));
    }

    #[test]
    fn result_actual_catches_errors_behind_options() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(error_record("ServiceError"));
        contract.functions.push(ir::definitions::FunctionDef {
            id: "load".into(),
            params: Vec::new(),
            returns: ir::definitions::ReturnDef::Result {
                ok: ir::types::TypeExpr::String,
                err: ir::types::TypeExpr::Option(Box::new(ir::types::TypeExpr::Record(
                    "ServiceError".into(),
                ))),
            },
            execution_kind: boltffi_ffi_rules::callable::ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let support = KmpSurfaceSupport::for_contract(&contract);
        assert!(function_supported(
            &contract.functions[0],
            &contract,
            &support
        ));

        let actual = KMPEmitter::render_kotlin_jvm_function_actual(
            &contract.functions[0],
            &contract,
            "com.example.demo.jvm",
        );

        assert!(actual.contains("catch (err: com.example.demo.jvm.ServiceError)"));
        assert!(actual.contains("catch (err: com.example.demo.jvm.FfiException)"));
    }

    #[test]
    fn emit_makes_empty_error_records_typed_catchable() {
        let mut contract = empty_contract();
        let mut empty_error = empty_record();
        empty_error.id = "EmptyError".into();
        empty_error.is_error = true;
        contract.catalog.insert_record(empty_error);
        contract.catalog.insert_custom(custom_type(
            "EmptyFailureWire",
            ir::types::TypeExpr::Record("EmptyError".into()),
        ));
        contract.catalog.insert_custom(custom_type(
            "EmptyFailure",
            ir::types::TypeExpr::Option(Box::new(ir::types::TypeExpr::Custom(
                "EmptyFailureWire".into(),
            ))),
        ));
        contract.functions.push(sync_function(
            "load",
            ir::definitions::ReturnDef::Result {
                ok: ir::types::TypeExpr::String,
                err: ir::types::TypeExpr::Custom("EmptyFailure".into()),
            },
        ));
        let abi = ir::Lowerer::new(&contract).to_abi_contract();

        let support = KmpSurfaceSupport::for_contract(&contract);
        assert!(function_supported(
            &contract.functions[0],
            &contract,
            &support
        ));

        let output = KMPEmitter::emit(
            &contract,
            &abi,
            KMPOptions {
                package_name: "com.example.demo".to_string(),
                module_name: "Demo".to_string(),
                min_sdk: 23,
                kotlin_options: KotlinOptions::default(),
            },
        );
        let common = output
            .files
            .iter()
            .find(|file| {
                file.relative_path.as_path()
                    == Path::new("src/commonMain/kotlin/com/example/demo/Demo.kt")
            })
            .expect("common source should be emitted");
        let jvm_actual = output
            .files
            .iter()
            .find(|file| {
                file.relative_path.as_path()
                    == Path::new("src/jvmMain/kotlin/com/example/demo/DemoJvmActual.kt")
            })
            .expect("jvm actual source should be emitted");
        let internal_jvm = output
            .files
            .iter()
            .find(|file| {
                file.relative_path.as_path()
                    == Path::new("src/jvmMain/kotlin/com/example/demo/jvm/Demo.kt")
            })
            .expect("jvm internal source should be emitted");

        assert!(
            common
                .contents
                .contains("object EmptyError : kotlin.Exception(\"\")")
        );
        assert!(
            jvm_actual
                .contents
                .contains("catch (err: com.example.demo.jvm.EmptyError)")
        );
        assert!(
            internal_jvm
                .contents
                .contains("object EmptyError : kotlin.Exception(\"\")")
        );
    }

    #[test]
    fn non_result_actual_converts_internal_ffi_exception() {
        let mut contract = empty_contract();
        contract.functions.push(ir::definitions::FunctionDef {
            id: "describe".into(),
            params: Vec::new(),
            returns: ir::definitions::ReturnDef::Value(ir::types::TypeExpr::String),
            execution_kind: boltffi_ffi_rules::callable::ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let actual = KMPEmitter::render_kotlin_jvm_function_actual(
            &contract.functions[0],
            &contract,
            "com.example.demo.jvm",
        );

        assert!(actual.contains("actual fun describe(): String {"));
        assert!(actual.contains("return com.example.demo.jvm.describe()"));
        assert!(actual.contains("catch (err: com.example.demo.jvm.FfiException)"));
        assert!(!actual.contains("actual fun describe(): String ="));
    }

    #[test]
    fn data_enum_payload_types_disambiguate_kotlin_builtins() {
        let enumeration = ir::definitions::EnumDef {
            id: "JsonValue".into(),
            repr: EnumRepr::Data {
                tag_type: ir::types::PrimitiveType::I32,
                variants: vec![
                    ir::definitions::DataVariant {
                        name: "String".into(),
                        discriminant: 0,
                        payload: VariantPayload::Tuple(vec![ir::types::TypeExpr::String]),
                        doc: None,
                    },
                    ir::definitions::DataVariant {
                        name: "Int".into(),
                        discriminant: 1,
                        payload: VariantPayload::Tuple(vec![ir::types::TypeExpr::Primitive(
                            ir::types::PrimitiveType::I32,
                        )]),
                        doc: None,
                    },
                    ir::definitions::DataVariant {
                        name: "ByteArray".into(),
                        discriminant: 2,
                        payload: VariantPayload::Tuple(vec![ir::types::TypeExpr::Vec(Box::new(
                            ir::types::TypeExpr::Primitive(ir::types::PrimitiveType::U8),
                        ))]),
                        doc: None,
                    },
                    ir::definitions::DataVariant {
                        name: "IntArray".into(),
                        discriminant: 3,
                        payload: VariantPayload::Tuple(vec![ir::types::TypeExpr::Vec(Box::new(
                            ir::types::TypeExpr::Primitive(ir::types::PrimitiveType::I32),
                        ))]),
                        doc: None,
                    },
                    ir::definitions::DataVariant {
                        name: "List".into(),
                        discriminant: 4,
                        payload: VariantPayload::Tuple(vec![ir::types::TypeExpr::Vec(Box::new(
                            ir::types::TypeExpr::String,
                        ))]),
                        doc: None,
                    },
                    ir::definitions::DataVariant {
                        name: "bolt_f_f_i_result".into(),
                        discriminant: 5,
                        payload: VariantPayload::Tuple(vec![ir::types::TypeExpr::Result {
                            ok: Box::new(ir::types::TypeExpr::String),
                            err: Box::new(ir::types::TypeExpr::String),
                        }]),
                        doc: None,
                    },
                ],
            },
            is_error: false,
            constructors: Vec::new(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        };

        let common = KMPEmitter::render_common_sealed_enum(
            &enumeration,
            "com.example.demo",
            &empty_contract(),
        );

        assert_eq!(
            NamingConvention::class_name("bolt_f_f_i_result"),
            "BoltFFIResult"
        );
        assert!(common.contains("data class String(val value0: kotlin.String) : JsonValue()"));
        assert!(common.contains("data class Int(val value0: kotlin.Int) : JsonValue()"));
        assert!(
            common.contains("data class ByteArray(val value0: kotlin.ByteArray) : JsonValue()")
        );
        assert!(common.contains("data class IntArray(val value0: kotlin.IntArray) : JsonValue()"));
        assert!(common.contains(
            "data class List(val value0: kotlin.collections.List<kotlin.String>) : JsonValue()"
        ));
        assert!(common.contains(
            "data class BoltFFIResult(val value0: com.example.demo.BoltFFIResult<kotlin.String, kotlin.String>) : JsonValue()"
        ));
    }

    #[test]
    fn default_platform_adapters_are_jvm_family_actuals() {
        let adapters = KMPEmitter::default_platform_adapters();

        assert_eq!(
            adapters,
            vec![KmpPlatformAdapter::jvm(), KmpPlatformAdapter::android()]
        );
        assert!(
            adapters
                .iter()
                .all(|adapter| matches!(adapter.backend, KmpActualBackend::KotlinJvm))
        );
    }

    #[test]
    fn surfaces_render_common_once_and_platform_actuals_separately() {
        let adapters = KMPEmitter::default_platform_adapters();
        let rendered = KMPEmitter::render_surfaces(
            &empty_contract(),
            "com.example.demo",
            "com.example.demo.jvm",
            &adapters,
        );

        assert!(rendered.common.contains("package com.example.demo"));
        assert_eq!(rendered.platform_actuals.len(), 2);
        assert_eq!(rendered.platform_actuals[0].adapter.source_set, "jvmMain");
        assert_eq!(
            rendered.platform_actuals[1].adapter.source_set,
            "androidMain"
        );
        assert!(
            rendered
                .platform_actuals
                .iter()
                .all(|actual| actual.contents.contains("package com.example.demo"))
        );
    }

    #[test]
    fn unsigned_enum_discriminants_render_as_valid_signed_kotlin_literals() {
        assert_eq!(
            enum_literal(u32::MAX as i128, ir::types::PrimitiveType::U32),
            "(4294967295L).toInt()"
        );
        assert_eq!(
            enum_literal(u64::MAX as i128, ir::types::PrimitiveType::U64),
            "(18446744073709551615uL).toLong()"
        );
    }
}
