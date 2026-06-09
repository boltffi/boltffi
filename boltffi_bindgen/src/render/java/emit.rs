use crate::ir::abi::AbiContract;
use crate::ir::contract::FfiContract;
use crate::ir::ids::BuiltinId;
use crate::ir::ops::{ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq};
use crate::ir::types::{PrimitiveType, TypeExpr};
use boltffi_ffi_rules::transport::EnumTagStrategy;

use crate::ir::codec::{EnumLayout, VecLayout};

use super::JavaOptions;
use super::lower::JavaLowerer;
use super::mappings;
use super::names::NamingConvention;
use super::plan::JavaEnumKind;
use super::templates::{
    CStyleEnumTemplate, CallbackCallbacksTemplate, CallbackTraitTemplate, ClassTemplate,
    ClosureCallbacksTemplate, ClosureTemplate, DataEnumAbstractTemplate, DataEnumSealedTemplate,
    ErrorEnumTemplate, FunctionsTemplate, NativeTemplate, PreambleTemplate, RecordTemplate,
    ResultTemplate,
};
use askama::Template;

pub struct JavaFile {
    pub file_name: String,
    pub source: String,
}

pub struct JavaOutput {
    pub files: Vec<JavaFile>,
    pub class_name: String,
    pub package_path: String,
}

pub struct JavaEmitter;

impl JavaEmitter {
    pub fn emit(
        ffi: &FfiContract,
        abi: &AbiContract,
        package_name: String,
        module_name: String,
        options: JavaOptions,
    ) -> JavaOutput {
        let lowerer = JavaLowerer::new(ffi, abi, package_name, module_name, options);
        let module = lowerer.module();
        let package_path = module.package_path();
        let class_name = module.class_name.clone();

        let mut files = Vec::new();

        for enumeration in &module.enums {
            let source = match enumeration.kind {
                JavaEnumKind::CStyle => {
                    let template = CStyleEnumTemplate {
                        enumeration,
                        package_name: &module.package_name,
                    };
                    template.render().expect("c-style enum template failed")
                }
                JavaEnumKind::Error => {
                    let template = ErrorEnumTemplate {
                        enumeration,
                        package_name: &module.package_name,
                    };
                    template.render().expect("error enum template failed")
                }
                JavaEnumKind::SealedInterface => {
                    let template = DataEnumSealedTemplate {
                        enumeration,
                        package_name: &module.package_name,
                    };
                    template.render().expect("sealed enum template failed")
                }
                JavaEnumKind::AbstractClass => {
                    let template = DataEnumAbstractTemplate {
                        enumeration,
                        package_name: &module.package_name,
                    };
                    template.render().expect("abstract enum template failed")
                }
                JavaEnumKind::ErrorAbstractClass => {
                    let template = DataEnumAbstractTemplate {
                        enumeration,
                        package_name: &module.package_name,
                    };
                    template
                        .render()
                        .expect("error abstract enum template failed")
                }
            };
            files.push(JavaFile {
                file_name: format!("{}.java", enumeration.class_name),
                source,
            });
        }

        for record in &module.records {
            let record_template = RecordTemplate {
                record,
                package_name: &module.package_name,
            };
            files.push(JavaFile {
                file_name: format!("{}.java", record.class_name),
                source: record_template.render().expect("record template failed"),
            });
        }

        for class in &module.classes {
            let class_template = ClassTemplate {
                class,
                package_name: &module.package_name,
                async_mode: &module.async_mode,
            };
            files.push(JavaFile {
                file_name: format!("{}.java", class.class_name),
                source: class_template.render().expect("class template failed"),
            });
        }

        let closure_files = module.closures.iter().flat_map(|closure| {
            let pkg = &module.package_name;
            [
                JavaFile {
                    file_name: format!("{}.java", closure.interface_name),
                    source: ClosureTemplate {
                        closure,
                        package_name: pkg,
                    }
                    .render()
                    .expect("closure interface template failed"),
                },
                JavaFile {
                    file_name: format!("{}.java", closure.callbacks_class_name),
                    source: ClosureCallbacksTemplate {
                        closure,
                        package_name: pkg,
                    }
                    .render()
                    .expect("closure callbacks template failed"),
                },
            ]
        });

        let callback_files = module.callbacks.iter().flat_map(|callback| {
            let pkg = &module.package_name;
            [
                JavaFile {
                    file_name: format!("{}.java", callback.interface_name),
                    source: CallbackTraitTemplate {
                        callback,
                        package_name: pkg,
                    }
                    .render()
                    .expect("callback trait template failed"),
                },
                JavaFile {
                    file_name: format!("{}.java", callback.callbacks_class_name()),
                    source: CallbackCallbacksTemplate {
                        callback,
                        package_name: pkg,
                    }
                    .render()
                    .expect("callback callbacks template failed"),
                },
            ]
        });

        files.extend(closure_files.chain(callback_files));

        files.push(JavaFile {
            file_name: "BoltFFIResult.java".to_string(),
            source: ResultTemplate {
                package_name: &module.package_name,
            }
            .render()
            .expect("result template failed"),
        });

        let mut main_source = String::new();

        let preamble = PreambleTemplate { module: &module };
        main_source.push_str(&preamble.render().expect("preamble template failed"));

        let native = NativeTemplate { module: &module };
        main_source.push_str(&native.render().expect("native template failed"));

        main_source.push('\n');

        let functions = FunctionsTemplate { module: &module };
        main_source.push_str(&functions.render().expect("functions template failed"));

        files.push(JavaFile {
            file_name: format!("{}.java", class_name),
            source: main_source,
        });

        JavaOutput {
            files,
            class_name,
            package_path,
        }
    }
}

fn render_value(expr: &ValueExpr) -> String {
    match expr {
        ValueExpr::Instance => String::new(),
        ValueExpr::Var(name) => name.clone(),
        ValueExpr::Named(name) => NamingConvention::field_name(name),
        ValueExpr::Field(parent, field) => {
            let parent_str = render_value(parent);
            let field_str = NamingConvention::field_name(field.as_str());
            if parent_str.is_empty() {
                field_str
            } else {
                format!("{}.{}", parent_str, field_str)
            }
        }
    }
}

#[derive(Default)]
struct JavaEmitContext {
    read_lambda_index: usize,
    write_loop_index: usize,
    size_lambda_index: usize,
}

impl JavaEmitContext {
    fn next_read_lambda(&mut self) -> String {
        let next_index = self.read_lambda_index;
        self.read_lambda_index += 1;
        format!("readIndex{}", next_index)
    }

    fn next_write_loop_var(&mut self) -> String {
        let next_index = self.write_loop_index;
        self.write_loop_index += 1;
        format!("item{}", next_index)
    }

    fn next_size_lambda(&mut self) -> String {
        let next_index = self.size_lambda_index;
        self.size_lambda_index += 1;
        format!("sizeItem{}", next_index)
    }
}

pub fn primitive_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "readBool",
        PrimitiveType::I8 | PrimitiveType::U8 => "readI8",
        PrimitiveType::I16 | PrimitiveType::U16 => "readI16",
        PrimitiveType::I32 | PrimitiveType::U32 => "readI32",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "readI64"
        }
        PrimitiveType::F32 => "readF32",
        PrimitiveType::F64 => "readF64",
    }
}

fn primitive_write_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "writeBool",
        PrimitiveType::I8 | PrimitiveType::U8 => "writeI8",
        PrimitiveType::I16 | PrimitiveType::U16 => "writeI16",
        PrimitiveType::I32 | PrimitiveType::U32 => "writeI32",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "writeI64"
        }
        PrimitiveType::F32 => "writeF32",
        PrimitiveType::F64 => "writeF64",
    }
}

pub fn emit_reader_read(seq: &ReadSeq) -> String {
    let mut context = JavaEmitContext::default();
    emit_reader_read_with_context(seq, &mut context)
}

fn emit_reader_read_with_context(seq: &ReadSeq, context: &mut JavaEmitContext) -> String {
    let op = seq.ops.first().expect("read ops");
    match op {
        ReadOp::Primitive { primitive, .. } => {
            format!("reader.{}()", primitive_read_method(*primitive))
        }
        ReadOp::String { .. } => "reader.readString()".to_string(),
        ReadOp::Builtin { id, .. } => {
            format!("reader.{}()", mappings::java_builtin_read_method(id))
        }
        ReadOp::Record { id, .. } => {
            format!(
                "{}.decode(reader)",
                NamingConvention::class_name(id.as_str())
            )
        }
        ReadOp::Enum { id, layout, .. } => match layout {
            EnumLayout::CStyle {
                tag_type,
                tag_strategy,
                ..
            } => match tag_strategy {
                EnumTagStrategy::Discriminant => {
                    format!(
                        "{}.fromValue(reader.{}())",
                        NamingConvention::class_name(id.as_str()),
                        primitive_read_method(*tag_type),
                    )
                }
                EnumTagStrategy::OrdinalIndex => {
                    format!(
                        "{}.fromTag(reader.readI32())",
                        NamingConvention::class_name(id.as_str()),
                    )
                }
            },
            EnumLayout::Data { .. } | EnumLayout::Recursive => {
                format!(
                    "{}.decode(reader)",
                    NamingConvention::class_name(id.as_str())
                )
            }
        },
        ReadOp::Option { some, .. } => {
            let inner = emit_reader_read_with_context(some, context);
            format!(
                "reader.readI8() == 0 ? java.util.Optional.empty() : java.util.Optional.ofNullable({})",
                inner,
            )
        }
        ReadOp::Vec {
            element_type,
            element,
            layout,
            ..
        } => emit_reader_vec_with_context(element_type, element, layout, context),
        ReadOp::Result { ok, err, .. } => {
            let ok_expr = emit_reader_read_or_null(ok, context);
            let err_expr = emit_reader_read_or_null(err, context);
            format!(
                "reader.readI8() == 0 ? BoltFFIResult.ok({}) : BoltFFIResult.err({})",
                ok_expr, err_expr,
            )
        }
        other => panic!("unsupported Java read op: {:?}", other),
    }
}

fn emit_reader_read_or_null(seq: &ReadSeq, context: &mut JavaEmitContext) -> String {
    if seq.ops.is_empty() {
        "null".to_string()
    } else {
        emit_reader_read_with_context(seq, context)
    }
}

fn emit_reader_vec_with_context(
    element_type: &TypeExpr,
    element: &ReadSeq,
    layout: &VecLayout,
    context: &mut JavaEmitContext,
) -> String {
    match layout {
        VecLayout::Blittable { .. } => match element_type {
            TypeExpr::Primitive(primitive) => {
                format!("reader.{}()", primitive_array_read_method(*primitive))
            }
            TypeExpr::Record(id) => {
                format!(
                    "{}.decodeBlittableVec(reader)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            _ => {
                let inner = emit_reader_read_with_context(element, context);
                let lambda_var = context.next_read_lambda();
                format!("reader.readList({} -> {})", lambda_var, inner)
            }
        },
        VecLayout::Encoded => match element_type {
            TypeExpr::Primitive(primitive) => {
                format!("reader.{}()", primitive_array_read_method(*primitive))
            }
            _ => {
                let inner = emit_reader_read_with_context(element, context);
                let lambda_var = context.next_read_lambda();
                format!("reader.readList({} -> {})", lambda_var, inner)
            }
        },
    }
}

pub fn primitive_array_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "readBooleanArray",
        PrimitiveType::I8 | PrimitiveType::U8 => "readByteArray",
        PrimitiveType::I16 | PrimitiveType::U16 => "readShortArray",
        PrimitiveType::I32 | PrimitiveType::U32 => "readIntArray",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "readLongArray"
        }
        PrimitiveType::F32 => "readFloatArray",
        PrimitiveType::F64 => "readDoubleArray",
    }
}

pub fn emit_write_expr(seq: &WriteSeq, writer_name: &str) -> String {
    let mut context = JavaEmitContext::default();
    emit_write_expr_with_context(seq, writer_name, &mut context)
}

fn emit_write_expr_with_context(
    seq: &WriteSeq,
    writer_name: &str,
    context: &mut JavaEmitContext,
) -> String {
    let op = seq.ops.first().expect("write ops");
    match op {
        WriteOp::Primitive { primitive, value } => {
            format!(
                "{}.{}({})",
                writer_name,
                primitive_write_method(*primitive),
                render_value(value)
            )
        }
        WriteOp::String { value } => {
            format!("{}.writeString({})", writer_name, render_value(value))
        }
        WriteOp::Builtin { id, value } => {
            format!(
                "{}.{}({})",
                writer_name,
                mappings::java_builtin_write_method(id),
                render_value(value)
            )
        }
        WriteOp::Option { value, some } => {
            let option_expr = render_value(value);
            let inner = emit_write_expr_with_context(some, writer_name, context);
            let some_value_expr = format!("({}).get()", option_expr);
            let remapped_inner = replace_identifier_occurrences(&inner, "v", &some_value_expr);
            format!(
                "if (({}).isPresent()) {{ {}.writeI8((byte)1); {}; }} else {{ {}.writeI8((byte)0); }}",
                option_expr, writer_name, remapped_inner, writer_name,
            )
        }
        WriteOp::Record { value, .. } => {
            format!("{}.wireEncodeTo({})", render_value(value), writer_name)
        }
        WriteOp::Enum { value, layout, .. } => match layout {
            EnumLayout::CStyle {
                tag_type,
                tag_strategy,
                ..
            } => match tag_strategy {
                EnumTagStrategy::Discriminant => {
                    format!(
                        "{}.{}({}.value)",
                        writer_name,
                        primitive_write_method(*tag_type),
                        render_value(value),
                    )
                }
                EnumTagStrategy::OrdinalIndex => {
                    format!(
                        "{}.writeI32({}.wireTag())",
                        writer_name,
                        render_value(value),
                    )
                }
            },
            EnumLayout::Data { .. } | EnumLayout::Recursive => {
                format!("{}.wireEncodeTo({})", render_value(value), writer_name)
            }
        },
        WriteOp::Result { value, ok, err } => {
            let result_expr = render_value(value);
            let ok_value_expr = format!("({}).okValue()", result_expr);
            let err_value_expr = format!("({}).errValue()", result_expr);
            let ok_payload =
                emit_result_branch_write(ok, writer_name, context, "okVal", &ok_value_expr);
            let err_payload =
                emit_result_branch_write(err, writer_name, context, "errVal", &err_value_expr);
            format!(
                "if (({}).isOk()) {{ {}.writeI8((byte)0);{} }} else {{ {}.writeI8((byte)1);{} }}",
                result_expr, writer_name, ok_payload, writer_name, err_payload,
            )
        }
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout,
        } => emit_write_vec_with_context(
            writer_name,
            &render_value(value),
            element_type,
            element,
            layout,
            context,
        ),
        other => panic!("unsupported Java write op: {:?}", other),
    }
}

fn emit_result_branch_write(
    seq: &WriteSeq,
    writer_name: &str,
    context: &mut JavaEmitContext,
    binding: &str,
    replacement: &str,
) -> String {
    if seq.ops.is_empty() {
        String::new()
    } else {
        let inner = emit_write_expr_with_context(seq, writer_name, context);
        let remapped = replace_identifier_occurrences(&inner, binding, replacement);
        format!(" {remapped};")
    }
}

fn emit_write_vec_with_context(
    writer_name: &str,
    value: &str,
    element_type: &TypeExpr,
    element: &WriteSeq,
    layout: &VecLayout,
    context: &mut JavaEmitContext,
) -> String {
    match layout {
        VecLayout::Blittable { .. } => match element_type {
            TypeExpr::Primitive(primitive) => {
                format!(
                    "{}.{}({})",
                    writer_name,
                    primitive_array_write_method(*primitive),
                    value,
                )
            }
            TypeExpr::Record(id) => {
                format!(
                    "{}.encodeBlittableVec({}, {})",
                    NamingConvention::class_name(id.as_str()),
                    writer_name,
                    value,
                )
            }
            _ => {
                let inner = emit_write_expr_with_context(element, writer_name, context);
                let loop_var = context.next_write_loop_var();
                let remapped_inner = replace_identifier_occurrences(&inner, "item", &loop_var);
                let iter_type = java_type_for_iteration(element_type);
                format!(
                    "{}.writeI32({}.size()); for ({} {} : {}) {{ {}; }}",
                    writer_name, value, iter_type, loop_var, value, remapped_inner,
                )
            }
        },
        VecLayout::Encoded => match element_type {
            TypeExpr::Primitive(primitive) => {
                format!(
                    "{}.{}({})",
                    writer_name,
                    primitive_array_write_method(*primitive),
                    value,
                )
            }
            _ => {
                let inner = emit_write_expr_with_context(element, writer_name, context);
                let loop_var = context.next_write_loop_var();
                let remapped_inner = replace_identifier_occurrences(&inner, "item", &loop_var);
                let iter_type = java_type_for_iteration(element_type);
                format!(
                    "{}.writeI32({}.size()); for ({} {} : {}) {{ {}; }}",
                    writer_name, value, iter_type, loop_var, value, remapped_inner,
                )
            }
        },
    }
}

fn java_type_for_iteration(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Primitive(primitive) => mappings::java_boxed_type(*primitive).to_string(),
        TypeExpr::String => "String".to_string(),
        TypeExpr::Builtin(id) => mappings::java_builtin_type(id).to_string(),
        TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
        TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
        TypeExpr::Option(inner) => {
            format!("java.util.Optional<{}>", java_type_for_iteration(inner))
        }
        TypeExpr::Result { ok, err } => {
            format!(
                "BoltFFIResult<{}, {}>",
                java_type_for_iteration(ok),
                java_type_for_iteration(err)
            )
        }
        TypeExpr::Vec(inner) => match inner.as_ref() {
            TypeExpr::Primitive(primitive) => {
                mappings::java_primitive_array_type(*primitive).to_string()
            }
            _ => format!("java.util.List<{}>", java_type_for_iteration(inner)),
        },
        TypeExpr::Void => "Void".to_string(),
        _ => "Object".to_string(),
    }
}

fn primitive_array_write_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "writeBooleanArray",
        PrimitiveType::I8 | PrimitiveType::U8 => "writeByteArray",
        PrimitiveType::I16 | PrimitiveType::U16 => "writeShortArray",
        PrimitiveType::I32 | PrimitiveType::U32 => "writeIntArray",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "writeLongArray"
        }
        PrimitiveType::F32 => "writeFloatArray",
        PrimitiveType::F64 => "writeDoubleArray",
    }
}

pub fn emit_size_expr(size: &SizeExpr) -> String {
    let mut context = JavaEmitContext::default();
    emit_size_expr_with_context(size, &mut context)
}

fn emit_size_expr_with_context(size: &SizeExpr, context: &mut JavaEmitContext) -> String {
    match size {
        SizeExpr::Fixed(value) => value.to_string(),
        SizeExpr::StringLen(value) => {
            format!("(4 + ({}).length() * 3)", render_value(value),)
        }
        SizeExpr::BytesLen(value) => {
            format!("{}.length", render_value(value))
        }
        SizeExpr::BuiltinSize { id, value } => emit_builtin_size(id, &render_value(value)),
        SizeExpr::WireSize { value, .. } => {
            format!("{}.wireEncodedSize()", render_value(value))
        }
        SizeExpr::OptionSize { value, inner } => {
            let option_expr = render_value(value);
            let inner_expr = emit_size_expr_with_context(inner, context);
            let some_value_expr = format!("({}).get()", option_expr);
            let remapped_inner = replace_identifier_occurrences(&inner_expr, "v", &some_value_expr);
            format!(
                "(1 + (({}).isPresent() ? ({}) : 0))",
                option_expr, remapped_inner,
            )
        }
        SizeExpr::ResultSize { value, ok, err } => {
            let result_expr = render_value(value);
            let ok_expr = emit_size_expr_with_context(ok, context);
            let err_expr = emit_size_expr_with_context(err, context);
            let ok_value_expr = format!("({}).okValue()", result_expr);
            let err_value_expr = format!("({}).errValue()", result_expr);
            let remapped_ok = replace_identifier_occurrences(&ok_expr, "okVal", &ok_value_expr);
            let remapped_err = replace_identifier_occurrences(&err_expr, "errVal", &err_value_expr);
            format!(
                "(1 + (({}).isOk() ? ({}) : ({})))",
                result_expr, remapped_ok, remapped_err,
            )
        }
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => emit_vec_size_expr_with_context(&render_value(value), inner, layout, context),
        SizeExpr::Sum(parts) => {
            let rendered = parts
                .iter()
                .map(|part| emit_size_expr_with_context(part, context))
                .collect::<Vec<_>>()
                .join(" + ");
            format!("({})", rendered)
        }
        other => panic!("unsupported Java size expr: {:?}", other),
    }
}

fn emit_builtin_size(id: &BuiltinId, value: &str) -> String {
    match id.as_str() {
        "Url" => format!("(({}).toString().length() * 3)", value),
        _ => panic!("unsupported Java builtin size: {:?}", id),
    }
}

fn emit_vec_size_expr_with_context(
    value: &str,
    inner: &SizeExpr,
    layout: &VecLayout,
    context: &mut JavaEmitContext,
) -> String {
    match layout {
        VecLayout::Blittable { element_size } => {
            format!("(4 + {} * {})", emit_vec_length_expr(value), element_size)
        }
        VecLayout::Encoded => {
            let inner_expr = emit_size_expr_with_context(inner, context);
            if inner_expr.contains("item") {
                let lambda_var = context.next_size_lambda();
                let remapped_inner =
                    replace_identifier_occurrences(&inner_expr, "item", &lambda_var);
                format!(
                    "WireWriter.listWireSize({}, {} -> {})",
                    value, lambda_var, remapped_inner,
                )
            } else {
                format!("(4 + {} * {})", emit_vec_length_expr(value), inner_expr)
            }
        }
    }
}

fn emit_vec_length_expr(value: &str) -> String {
    format!("WireWriter.vecLength({})", value)
}

fn replace_identifier_occurrences(expression: &str, identifier: &str, replacement: &str) -> String {
    if identifier.is_empty() {
        return expression.to_string();
    }

    let mut result = String::with_capacity(expression.len());
    let mut cursor = 0;

    while let Some(relative_index) = expression[cursor..].find(identifier) {
        let start = cursor + relative_index;
        let end = start + identifier.len();
        let previous = expression[..start].chars().next_back();
        let next = expression[end..].chars().next();
        let previous_is_identifier = previous.map(is_identifier_char).unwrap_or(false);
        let next_is_identifier = next.map(is_identifier_char).unwrap_or(false);

        if previous_is_identifier || next_is_identifier {
            result.push_str(&expression[cursor..end]);
            cursor = end;
        } else {
            result.push_str(&expression[cursor..start]);
            result.push_str(replacement);
            cursor = end;
        }
    }

    result.push_str(&expression[cursor..]);
    result
}

fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{
        CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, FieldDef,
        FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef,
    };
    use crate::ir::ids::{
        CallbackId, ClassId, FieldName, FunctionId, MethodId, ParamName, RecordId,
    };
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use crate::render::java::JavaVersion;
    use boltffi_ffi_rules::callable::ExecutionKind;
    use std::env;
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    fn class_def(id: &str, constructors: Vec<ConstructorDef>, methods: Vec<MethodDef>) -> ClassDef {
        ClassDef {
            id: ClassId::from(id),
            constructors,
            methods,
            streams: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn default_ctor(params: Vec<ParamDef>) -> ConstructorDef {
        ConstructorDef::Default {
            params,
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn param(name: &str, type_expr: TypeExpr) -> ParamDef {
        ParamDef {
            name: ParamName::from(name),
            type_expr,
            passing: ParamPassing::Value,
            doc: None,
        }
    }

    fn field(name: &str, type_expr: TypeExpr) -> FieldDef {
        FieldDef {
            name: FieldName::new(name),
            type_expr,
            doc: None,
            default: None,
        }
    }

    fn instance_method(name: &str, params: Vec<ParamDef>, returns: ReturnDef) -> MethodDef {
        MethodDef {
            id: MethodId::from(name),
            receiver: Receiver::RefSelf,
            params,
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    fn async_callback_method(
        name: &str,
        params: Vec<ParamDef>,
        returns: ReturnDef,
    ) -> CallbackMethodDef {
        CallbackMethodDef {
            id: MethodId::from(name),
            params,
            returns,
            execution_kind: ExecutionKind::Async,
            doc: None,
        }
    }

    #[test]
    fn emit_generates_class_file() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_class(class_def("counter", vec![default_ctor(vec![])], vec![]));
        let abi = IrLowerer::new(&contract).to_abi_contract();

        let output = JavaEmitter::emit(
            &contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            JavaOptions::default(),
        );

        let class_file = output
            .files
            .iter()
            .find(|file| file.file_name == "Counter.java");
        assert!(class_file.is_some());
        let class_source = class_file
            .map(|file| file.source.as_str())
            .expect("class file should exist");
        assert!(class_source.contains("implements AutoCloseable"));
    }

    #[test]
    fn emit_main_file_includes_class_native_declarations() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![])],
            vec![instance_method(
                "get",
                vec![param("slot", TypeExpr::Primitive(PrimitiveType::I32))],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            )],
        ));
        let abi = IrLowerer::new(&contract).to_abi_contract();

        let output = JavaEmitter::emit(
            &contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            JavaOptions::default(),
        );

        let main_file = output
            .files
            .iter()
            .find(|file| file.file_name == "Test.java")
            .expect("main file should exist");

        assert!(
            main_file
                .source
                .contains("static native void boltffi_counter_free(long handle);")
        );
        assert!(
            main_file
                .source
                .contains("static native long boltffi_counter_new();")
        );
        assert!(
            main_file
                .source
                .contains("static native int boltffi_counter_get(long handle, int slot);")
        );
    }

    #[test]
    fn emit_generated_java_compiles_with_javac_when_available() {
        if Command::new("javac").arg("-version").output().is_err() {
            return;
        }

        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![])],
            vec![instance_method(
                "get",
                vec![param("slot", TypeExpr::Primitive(PrimitiveType::I32))],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            )],
        ));
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = JavaEmitter::emit(
            &contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            JavaOptions::default(),
        );

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let tmp_root = env::temp_dir().join(format!("boltffi-java-{}", nanos));
        let package_dir = tmp_root.join(&output.package_path);
        fs::create_dir_all(&package_dir).expect("should create package directory");

        let source_paths: Vec<_> = output
            .files
            .iter()
            .map(|file| {
                let path = package_dir.join(&file.file_name);
                fs::write(&path, &file.source).expect("should write generated source");
                path
            })
            .collect();

        let status = Command::new("javac")
            .args(source_paths)
            .status()
            .expect("javac should execute");
        assert!(status.success());

        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn emit_value_type_default_constructor_collision_compiles_with_javac_when_available() {
        if Command::new("javac").arg("-version").output().is_err() {
            return;
        }

        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("point"),
            fields: vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F64)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
            constructors: vec![default_ctor(vec![
                param("x", TypeExpr::Primitive(PrimitiveType::F64)),
                param("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ])],
            methods: vec![static_method(
                "new_",
                vec![
                    param("x", TypeExpr::Primitive(PrimitiveType::F64)),
                    param("y", TypeExpr::Primitive(PrimitiveType::F64)),
                ],
                ReturnDef::Value(TypeExpr::Record(RecordId::new("point"))),
            )],
            doc: None,
            deprecated: None,
        });
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = JavaEmitter::emit(
            &contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            JavaOptions::default(),
        );

        let point_file = output
            .files
            .iter()
            .find(|file| file.file_name == "Point.java")
            .expect("record file should exist");
        assert_eq!(point_file.source.matches("static Point _new").count(), 1);

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let tmp_root = env::temp_dir().join(format!("boltffi-java-keyword-collision-{}", nanos));
        let package_dir = tmp_root.join(&output.package_path);
        fs::create_dir_all(&package_dir).expect("should create package directory");

        let source_paths: Vec<_> = output
            .files
            .iter()
            .map(|file| {
                let path = package_dir.join(&file.file_name);
                fs::write(&path, &file.source).expect("should write generated source");
                path
            })
            .collect();

        let status = Command::new("javac")
            .args(source_paths)
            .status()
            .expect("javac should execute");
        assert!(
            status.success(),
            "keyword-colliding Java sources should compile"
        );

        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn emit_callback_return_java_compiles_with_javac_when_available() {
        if Command::new("javac").arg("-version").output().is_err() {
            return;
        }

        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("Listener"),
            methods: vec![CallbackMethodDef {
                id: MethodId::from("on_value"),
                params: vec![param("value", TypeExpr::Primitive(PrimitiveType::I32))],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        });
        contract.functions.push(FunctionDef {
            id: FunctionId::new("next_listener"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::Callback(CallbackId::new("Listener"))),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = JavaEmitter::emit(
            &contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            JavaOptions::default(),
        );

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let tmp_root = env::temp_dir().join(format!("boltffi-java-callbacks-{}", nanos));
        let package_dir = tmp_root.join(&output.package_path);
        fs::create_dir_all(&package_dir).expect("should create package directory");

        let source_paths: Vec<_> = output
            .files
            .iter()
            .map(|file| {
                let path = package_dir.join(&file.file_name);
                fs::write(&path, &file.source).expect("should write generated source");
                path
            })
            .collect();

        let status = Command::new("javac")
            .args(source_paths)
            .status()
            .expect("javac should execute");
        assert!(status.success());

        let _ = fs::remove_dir_all(tmp_root);
    }

    fn async_function(name: &str, params: Vec<ParamDef>, returns: ReturnDef) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(name),
            params,
            returns,
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        }
    }

    fn async_instance_method(name: &str, params: Vec<ParamDef>, returns: ReturnDef) -> MethodDef {
        MethodDef {
            id: MethodId::from(name),
            receiver: Receiver::RefSelf,
            params,
            returns,
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        }
    }

    fn static_method(name: &str, params: Vec<ParamDef>, returns: ReturnDef) -> MethodDef {
        MethodDef {
            id: MethodId::from(name),
            receiver: Receiver::Static,
            params,
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn emit_async_java_compiles_with_javac_when_available() {
        if Command::new("javac").arg("-version").output().is_err() {
            return;
        }

        let mut contract = empty_contract();

        contract.functions.push(async_function(
            "fetch_name",
            vec![param("id", TypeExpr::Primitive(PrimitiveType::I64))],
            ReturnDef::Value(TypeExpr::String),
        ));
        contract
            .functions
            .push(async_function("fire_event", vec![], ReturnDef::Void));
        contract.functions.push(async_function(
            "get_count",
            vec![],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        contract.catalog.insert_class(class_def(
            "session",
            vec![default_ctor(vec![])],
            vec![
                async_instance_method("load", vec![], ReturnDef::Value(TypeExpr::String)),
                async_instance_method("save", vec![], ReturnDef::Void),
                instance_method(
                    "get_id",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I64)),
                ),
                static_method(
                    "count",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                ),
            ],
        ));

        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = JavaEmitter::emit(
            &contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            JavaOptions::default(),
        );

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let tmp_root = env::temp_dir().join(format!("boltffi-java-async-{}", nanos));
        let package_dir = tmp_root.join(&output.package_path);
        fs::create_dir_all(&package_dir).expect("should create package directory");

        let source_paths: Vec<_> = output
            .files
            .iter()
            .map(|file| {
                let path = package_dir.join(&file.file_name);
                fs::write(&path, &file.source).expect("should write generated source");
                path
            })
            .collect();

        let status = Command::new("javac")
            .args(&source_paths)
            .status()
            .expect("javac should execute");

        if !status.success() {
            for file in &output.files {
                eprintln!("=== {} ===\n{}", file.file_name, file.source);
            }
        }
        assert!(status.success(), "async Java sources should compile");

        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn emit_async_vt_java_compiles_with_javac_when_available() {
        if Command::new("javac").arg("-version").output().is_err() {
            return;
        }

        let mut contract = empty_contract();

        contract.functions.push(async_function(
            "fetch_name",
            vec![param("id", TypeExpr::Primitive(PrimitiveType::I64))],
            ReturnDef::Value(TypeExpr::String),
        ));
        contract
            .functions
            .push(async_function("fire_event", vec![], ReturnDef::Void));
        contract.functions.push(async_function(
            "get_count",
            vec![],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        contract.catalog.insert_class(class_def(
            "session",
            vec![default_ctor(vec![])],
            vec![
                async_instance_method("load", vec![], ReturnDef::Value(TypeExpr::String)),
                async_instance_method("save", vec![], ReturnDef::Void),
                instance_method(
                    "get_id",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I64)),
                ),
                static_method(
                    "count",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                ),
            ],
        ));

        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = JavaEmitter::emit(
            &contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            JavaOptions {
                min_java_version: JavaVersion::JAVA_21,
                ..Default::default()
            },
        );

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let tmp_root = env::temp_dir().join(format!("boltffi-java-async-vt-{}", nanos));
        let package_dir = tmp_root.join(&output.package_path);
        fs::create_dir_all(&package_dir).expect("should create package directory");

        let source_paths: Vec<_> = output
            .files
            .iter()
            .map(|file| {
                let path = package_dir.join(&file.file_name);
                fs::write(&path, &file.source).expect("should write generated source");
                path
            })
            .collect();

        let status = Command::new("javac")
            .args(&source_paths)
            .status()
            .expect("javac should execute");

        if !status.success() {
            for file in &output.files {
                eprintln!("=== {} ===\n{}", file.file_name, file.source);
            }
        }
        assert!(
            status.success(),
            "async virtual-thread Java sources should compile"
        );

        let _ = fs::remove_dir_all(tmp_root);
    }

    #[test]
    fn emit_async_callback_bridge_normalizes_throwable_paths() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("listener"),
            methods: vec![async_callback_method(
                "fetch",
                vec![param("id", TypeExpr::Primitive(PrimitiveType::I32))],
                ReturnDef::Result {
                    ok: TypeExpr::Primitive(PrimitiveType::I32),
                    err: TypeExpr::String,
                },
            )],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = JavaEmitter::emit(
            &contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            JavaOptions::default(),
        );

        let callback_file = output
            .files
            .iter()
            .find(|file| file.file_name == "ListenerCallbacks.java")
            .expect("callback bridge should be generated");

        assert!(
            callback_file
                .source
                .contains("future.completeExceptionally(throwable);")
        );
        assert!(
            callback_file
                .source
                .contains("Throwable callbackThrowable = unwrapAsyncThrowable(throwable);")
        );
        assert!(
            callback_file
                .source
                .contains("callbackThrowable instanceof RuntimeException")
        );
    }
}
