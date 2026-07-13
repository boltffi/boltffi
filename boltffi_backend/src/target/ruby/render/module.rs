//! Ruby native extension module assembly.

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    Bindings, ClassDecl, DeclarationRef, DirectValueType, EnumDecl, ErrorDecl, ExecutionDecl,
    IncomingParam, IntoRust, Native, OutOfRust, ParamDecl, ParamPlan, Receive, ReturnPlan, TypeRef,
    native,
};

use crate::{
    bridge::c::{CBridgeContract, Function as CFunction, Type},
    core::{
        Emitted, Error, FileLayout, FilePath, GeneratedFile, GeneratedOutput, RenderedDeclaration,
        Result,
    },
    target::ruby::name_style::Name,
};

#[derive(AskamaTemplate)]
#[template(path = "target/ruby/native_module.c.txt", escape = "none")]
struct NativeModuleTemplate {
    module_name: String,
    init_name: String,
    functions: Vec<ModuleFunction>,
    body: String,
    init_body: String,
    needs_bool_helper: bool,
    needs_string_helper: bool,
    needs_bytes_helper: bool,
    ractor_safe: bool,
}

#[derive(AskamaTemplate)]
#[template(path = "target/ruby/lib.rb.txt", escape = "none")]
struct LibTemplate {
    module_name: String,
    crate_stem: String,
    enums: Vec<ModuleEnum>,
    functions: Vec<ModuleFunction>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/ruby/gemspec.txt", escape = "none")]
struct GemspecTemplate {
    gem_name: String,
    crate_stem: String,
    version: String,
}

#[derive(AskamaTemplate)]
#[template(path = "target/ruby/extconf.rb.txt", escape = "none")]
struct ExtconfTemplate {
    crate_stem: String,
}

pub struct NativeExtension<'bridge, 'decl> {
    _bridge: &'bridge CBridgeContract,
    declarations: Vec<RenderedDeclaration<'decl, Native>>,
    package_name: String,
    version: String,
    ractor_safe: bool,
}

#[derive(Clone)]
struct ModuleFunction {
    ruby_name: String,
    wrapper: String,
    registration_arity: i32,
}

struct ModuleEnum {
    class_name: String,
    variants: Vec<ModuleEnumVariant>,
}

struct ModuleEnumVariant {
    const_name: String,
    discriminant: i128,
}

impl<'bridge, 'decl> NativeExtension<'bridge, 'decl> {
    pub fn new(
        bindings: &Bindings<Native>,
        bridge: &'bridge CBridgeContract,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
        ractor_safe: bool,
    ) -> Self {
        Self {
            _bridge: bridge,
            declarations,
            package_name: bindings.package().name().as_path_string(),
            version: bindings.package().version().unwrap_or("0.1.0").to_owned(),
            ractor_safe,
        }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let mut enums = Vec::new();
        let mut functions = Vec::new();
        let mut chunks = Vec::new();
        let mut init_chunks = Vec::new();
        for declaration in self.declarations {
            match declaration.declaration() {
                DeclarationRef::Enum(EnumDecl::CStyle(enumeration)) => {
                    enums.push(ModuleEnum {
                        class_name: Name::new(enumeration.name()).class(),
                        variants: enumeration
                            .variants()
                            .iter()
                            .map(|variant| ModuleEnumVariant {
                                const_name: Name::new(variant.name()).constant(),
                                discriminant: variant.discriminant().get(),
                            })
                            .collect(),
                    });
                }
                DeclarationRef::Record(_) => {
                    let emitted = declaration.emitted().primary_chunk().as_str().to_owned();
                    if let Some(rendered) = parse_record_rendering(&emitted) {
                        chunks.push(rendered.body);
                        init_chunks.push(rendered.init);
                    }
                }
                DeclarationRef::Function(_) => {
                    let emitted = declaration.emitted().primary_chunk().as_str().to_owned();
                    if let Some(meta) = parse_function_meta(&emitted) {
                        functions.push(meta);
                    }
                    chunks.push(emitted);
                }
                DeclarationRef::Class(class) => {
                    if let Some(rendered) = render_class(class, self._bridge) {
                        chunks.push(rendered.body);
                        init_chunks.push(rendered.init);
                    }
                }
                _ => {}
            }
        }
        // Match the bindgen crate-stem normalization exactly: collapse both
        // `::` and `-` to `_`. If these disagree, the C header lands in one
        // `ext/<stem>` directory while `_native.c` includes it from another.
        let crate_stem = self.package_name.replace("::", "_").replace('-', "_");
        let module_name = upper_camel(&crate_stem);
        let version = self.version;
        let body = chunks.join("\n");
        let source = NativeModuleTemplate {
            module_name: module_name.clone(),
            init_name: format!("{crate_stem}_native"),
            functions: functions.clone(),
            needs_bool_helper: body.contains("boltffi_ruby_bool("),
            needs_string_helper: body.contains("boltffi_ruby_decode_string("),
            needs_bytes_helper: body.contains("boltffi_ruby_decode_bytes("),
            ractor_safe: self.ractor_safe,
            body,
            init_body: init_chunks.join("\n"),
        }
        .render()?;
        let mut output = FileLayout::single(FilePath::new(format!("ext/{crate_stem}/_native.c"))?)
            .assemble([Emitted::primary(source)])?;
        output.append(GeneratedOutput::new(
            vec![
                GeneratedFile::new(
                    FilePath::new(format!("lib/{crate_stem}.rb"))?,
                    LibTemplate {
                        module_name,
                        crate_stem: crate_stem.clone(),
                        enums,
                        functions,
                    }
                    .render()?,
                ),
                GeneratedFile::new(
                    FilePath::new(format!("ext/{crate_stem}/extconf.rb"))?,
                    ExtconfTemplate {
                        crate_stem: crate_stem.clone(),
                    }
                    .render()?,
                ),
                GeneratedFile::new(
                    FilePath::new(format!("{crate_stem}.gemspec"))?,
                    GemspecTemplate {
                        gem_name: crate_stem.clone(),
                        crate_stem,
                        version,
                    }
                    .render()?,
                ),
            ],
            Vec::new(),
        ));
        Ok(output)
    }
}

fn parse_function_meta(source: &str) -> Option<ModuleFunction> {
    let line = source.lines().next()?;
    let mut parts = line
        .strip_prefix("/* boltffi:ruby:function ")?
        .strip_suffix(" */")?
        .split(' ');
    let ruby_name = parts.next()?.strip_prefix("name=")?.to_owned();
    let wrapper = parts.next()?.strip_prefix("wrapper=")?.to_owned();
    let _argc: usize = parts.next()?.strip_prefix("argc=")?.parse().ok()?;
    let registration_arity: i32 = parts.next()?.strip_prefix("arity=")?.parse().ok()?;
    Some(ModuleFunction {
        ruby_name,
        wrapper,
        registration_arity,
    })
}

fn upper_camel(input: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in input.chars() {
        if ch == '_' || ch == '-' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

struct RenderedClass {
    body: String,
    init: String,
}

struct RenderedRecord {
    body: String,
    init: String,
}

fn parse_record_rendering(source: &str) -> Option<RenderedRecord> {
    let start = source.find("/* boltffi:ruby:record:init ")?;
    let body = source[..start].to_owned();
    let init_source = &source[start..];
    let init_start = init_source.find("*/")? + 2;
    let init_end = init_source.find("/* boltffi:ruby:record:init:end */")?;
    Some(RenderedRecord {
        body,
        init: init_source[init_start..init_end]
            .trim_matches('\n')
            .to_owned()
            + "\n",
    })
}

/// Joins C source lines with newlines and a trailing newline. Each line carries
/// its own indentation, so the emitted C reads naturally instead of the
/// flush-left output that `\`-continued string literals produce (the backslash
/// escape strips the leading whitespace of the next source line).
fn c_lines<I>(lines: I) -> String
where
    I: IntoIterator<Item = String>,
{
    let mut out = lines.into_iter().collect::<Vec<_>>().join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn render_class(class: &ClassDecl<Native>, bridge: &CBridgeContract) -> Option<RenderedClass> {
    let class_name = Name::new(class.name()).class();
    let ident = Name::new(class.name()).function();
    let c_type = "uint64_t";
    let release = class.release().name().as_str();
    let mut body = String::new();
    let mut init = String::new();

    body.push_str(&c_lines([
        format!("typedef struct {{ {c_type} handle; }} boltffi_ruby_{ident};"),
        format!("static VALUE c_{ident};"),
        format!("static void boltffi_ruby_{ident}_free(void *ptr) {{"),
        format!("    boltffi_ruby_{ident} *wrapper = (boltffi_ruby_{ident} *)ptr;"),
        "    if (wrapper != NULL) {".to_owned(),
        "        uint64_t handle = wrapper->handle;".to_owned(),
        "        wrapper->handle = 0;".to_owned(),
        format!("        if (handle != 0) {{ {release}(handle); }}"),
        "        xfree(wrapper);".to_owned(),
        "    }".to_owned(),
        "}".to_owned(),
        format!("static const rb_data_type_t boltffi_ruby_{ident}_type = {{"),
        format!(
            "    \"{class_name}\", {{ NULL, boltffi_ruby_{ident}_free, NULL, NULL }}, NULL, NULL, RUBY_TYPED_FREE_IMMEDIATELY"
        ),
        "};".to_owned(),
        format!("static VALUE boltffi_ruby_{ident}_alloc(VALUE klass) {{"),
        format!("    boltffi_ruby_{ident} *wrapper;"),
        format!(
            "    VALUE obj = TypedData_Make_Struct(klass, boltffi_ruby_{ident}, &boltffi_ruby_{ident}_type, wrapper);"
        ),
        "    wrapper->handle = 0;".to_owned(),
        "    return obj;".to_owned(),
        "}".to_owned(),
        format!("static boltffi_ruby_{ident} *boltffi_ruby_{ident}_unwrap(VALUE self) {{"),
        format!("    boltffi_ruby_{ident} *wrapper;"),
        format!(
            "    TypedData_Get_Struct(self, boltffi_ruby_{ident}, &boltffi_ruby_{ident}_type, wrapper);"
        ),
        format!(
            "    if (wrapper == NULL || wrapper->handle == 0) rb_raise(rb_eRuntimeError, \"use of released {class_name}\");"
        ),
        "    return wrapper;".to_owned(),
        "}".to_owned(),
        format!("static VALUE boltffi_ruby_{ident}_close(VALUE self) {{"),
        format!("    boltffi_ruby_{ident} *wrapper;"),
        format!(
            "    TypedData_Get_Struct(self, boltffi_ruby_{ident}, &boltffi_ruby_{ident}_type, wrapper);"
        ),
        format!(
            "    if (wrapper == NULL || wrapper->handle == 0) rb_raise(rb_eRuntimeError, \"use of released {class_name}\");"
        ),
        "    uint64_t handle = wrapper->handle;".to_owned(),
        "    wrapper->handle = 0;".to_owned(),
        format!("    {release}(handle);"),
        "    return Qnil;".to_owned(),
        "}".to_owned(),
    ]));

    init.push_str(&format!(
        "    c_{ident} = rb_define_class_under(mod, \"{class_name}\", rb_cObject);\n    rb_define_alloc_func(c_{ident}, boltffi_ruby_{ident}_alloc);\n    rb_define_method(c_{ident}, \"close\", boltffi_ruby_{ident}_close, 0);\n"
    ));

    for initializer in class.initializers() {
        if !matches!(
            initializer.callable().execution(),
            ExecutionDecl::Synchronous(_)
        ) {
            continue;
        }
        let is_fallible = !matches!(initializer.callable().error(), ErrorDecl::None(_));
        let symbol = initializer.symbol().name().as_str();
        let Some(c_function) = bridge
            .functions()
            .iter()
            .find(|function| function.name() == symbol)
        else {
            continue;
        };
        let Ok(params) = render_params(initializer.callable().params()) else {
            continue;
        };
        let argc = params.len();
        let ruby_name = Name::new(initializer.name()).function();
        let call_args = params
            .iter()
            .map(|param| param.arg.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let wrapper = if ruby_name == "new" {
            format!("boltffi_ruby_{ident}_initialize")
        } else {
            format!("boltffi_ruby_{ident}_ctor_{ruby_name}")
        };
        body.push_str(&wrapper_open(&wrapper, argc));
        if ruby_name == "new" {
            body.push_str(&format!(
                "    boltffi_ruby_{ident} *existing;\n    TypedData_Get_Struct(self, boltffi_ruby_{ident}, &boltffi_ruby_{ident}_type, existing);\n    if (existing == NULL || existing->handle != 0) rb_raise(rb_eRuntimeError, \"{class_name} is already initialized\");\n"
            ));
        } else {
            // Allocate the wrapper (handle=0) before decoding params or calling
            // the native constructor. If any later step raises, the GC-managed
            // object owns nothing yet, so no native handle can leak. The handle
            // is assigned only after the native call succeeds.
            body.push_str(&format!(
                "    boltffi_ruby_{ident} *wrapper_data;\n    VALUE obj = TypedData_Make_Struct(self, boltffi_ruby_{ident}, &boltffi_ruby_{ident}_type, wrapper_data);\n    wrapper_data->handle = 0;\n"
            ));
        }
        for param in &params {
            body.push_str(&param.decl);
        }
        let cleanup = params
            .iter()
            .map(|param| param.cleanup.clone())
            .collect::<String>();
        if is_fallible {
            body.push_str("    uint64_t handle = 0;\n");
            let call = if call_args.is_empty() {
                format!("{symbol}(&handle)")
            } else {
                format!("{symbol}({call_args}, &handle)")
            };
            body.push_str(&format!("    FfiBuf_u8 err = {call};\n"));
            // Free input buffers now that the call has returned, before any
            // `rb_raise` longjmps past this frame.
            body.push_str(&cleanup);
            // On the error path `boltffi_ruby_decode_string` consumes `err`;
            // on the success path (`err.len == 0`) free it explicitly.
            body.push_str(&c_lines([
                "    if (err.len > 0) {".to_owned(),
                "        VALUE r_err = boltffi_ruby_decode_string(err);".to_owned(),
                "        rb_raise(rb_eRuntimeError, \"%s\", StringValueCStr(r_err));".to_owned(),
                "    }".to_owned(),
                "    boltffi_free_buf(err);".to_owned(),
            ]));
            if ruby_name == "new" {
                body.push_str(&format!(
                    "    boltffi_ruby_{ident} *wrapper_data;\n    TypedData_Get_Struct(self, boltffi_ruby_{ident}, &boltffi_ruby_{ident}_type, wrapper_data);\n    wrapper_data->handle = handle;\n    return self;\n}}\n"
                ));
            } else {
                // `wrapper_data`/`obj` were allocated in the non-new prologue.
                body.push_str("    wrapper_data->handle = handle;\n    return obj;\n}\n");
            }
        } else {
            let call = if call_args.is_empty() {
                format!("{symbol}()")
            } else {
                format!("{symbol}({call_args})")
            };
            if ruby_name == "new" {
                body.push_str(&format!(
                    "    boltffi_ruby_{ident} *wrapper_data;\n    TypedData_Get_Struct(self, boltffi_ruby_{ident}, &boltffi_ruby_{ident}_type, wrapper_data);\n    wrapper_data->handle = {call};\n{cleanup}    return self;\n}}\n"
                ));
            } else {
                // `wrapper_data`/`obj` were allocated in the non-new prologue;
                // the native call runs after allocation so no handle can leak.
                body.push_str(&format!(
                    "    wrapper_data->handle = {call};\n{cleanup}    return obj;\n}}\n"
                ));
            }
        }
        let arity = registration_arity(argc);
        if ruby_name == "new" {
            init.push_str(&format!(
                "    rb_define_method(c_{ident}, \"initialize\", {wrapper}, {arity});\n"
            ));
        } else {
            init.push_str(&format!(
                "    rb_define_singleton_method(c_{ident}, \"{ruby_name}\", {wrapper}, {arity});\n"
            ));
        }
        let _ = c_function;
    }

    for method in class.methods() {
        if !matches!(method.callable().execution(), ExecutionDecl::Synchronous(_)) {
            continue;
        }
        if !matches!(method.callable().error(), ErrorDecl::None(_)) {
            continue;
        }
        let symbol = method.target().name().as_str();
        let c_function = match bridge
            .functions()
            .iter()
            .find(|function| function.name() == symbol)
        {
            Some(function) => function,
            None => continue,
        };
        let params = match render_params(method.callable().params()) {
            Ok(params) => params,
            Err(_) => continue,
        };
        let Ok((decl, expr)) = render_return(c_function, method.callable().returns().plan()) else {
            continue;
        };
        let argc = params.len();
        let ruby_name = Name::new(method.name()).function();
        let wrapper = format!("boltffi_ruby_{ident}_{ruby_name}");
        let is_static = method.callable().receiver().is_none();
        body.push_str(&wrapper_open(&wrapper, argc));
        if !is_static {
            body.push_str(&format!(
                "    boltffi_ruby_{ident} *wrapper_data = boltffi_ruby_{ident}_unwrap(self);\n"
            ));
        }
        for param in &params {
            body.push_str(&param.decl);
        }
        let mut call_args = Vec::new();
        if !is_static {
            call_args.push("wrapper_data->handle".to_string());
        }
        call_args.extend(params.iter().map(|param| param.arg.clone()));
        let call = format!("{symbol}({})", call_args.join(", "));
        let cleanup = params
            .iter()
            .map(|param| param.cleanup.clone())
            .collect::<String>();
        body.push_str(&format!(
            "    {}\n{cleanup}    return {};\n}}\n",
            decl.replace("CALL", &call),
            expr
        ));
        let arity = registration_arity(argc);
        if is_static {
            init.push_str(&format!(
                "    rb_define_singleton_method(c_{ident}, \"{ruby_name}\", {wrapper}, {arity});\n"
            ));
        } else {
            init.push_str(&format!(
                "    rb_define_method(c_{ident}, \"{ruby_name}\", {wrapper}, {arity});\n"
            ));
        }
    }

    Some(RenderedClass { body, init })
}

struct RenderedParam {
    decl: String,
    arg: String,
    /// C statements (already indented) to run after the native call returns,
    /// e.g. freeing a heap wire buffer built for a slice argument.
    cleanup: String,
}

fn render_params(
    params: &[ParamDecl<Native, IntoRust>],
) -> std::result::Result<Vec<RenderedParam>, ()> {
    let variadic = is_variadic(params.len());
    params
        .iter()
        .enumerate()
        .map(|(index, param)| render_param(&arg_accessor(index, variadic), param))
        .collect()
}

fn render_param(
    accessor: &str,
    param: &ParamDecl<Native, IntoRust>,
) -> std::result::Result<RenderedParam, ()> {
    let name = Name::new(param.name()).function();
    match param.payload() {
        IncomingParam::Value(ParamPlan::Direct {
            ty: DirectValueType::Primitive(primitive),
            receive: Receive::ByValue,
        }) => {
            let decls = primitive_declarations(accessor, &name, *primitive).map_err(|_| ())?;
            Ok(RenderedParam {
                decl: indent_statements(&decls),
                arg: name,
                cleanup: String::new(),
            })
        }
        IncomingParam::Value(ParamPlan::Direct {
            ty: DirectValueType::Enum(_),
            receive: Receive::ByValue,
        }) => Ok(RenderedParam {
            decl: format!("    int32_t {name} = NUM2INT({accessor});\n"),
            arg: name,
            cleanup: String::new(),
        }),
        IncomingParam::Value(ParamPlan::Encoded {
            ty: TypeRef::String | TypeRef::Bytes,
            shape: native::BufferShape::Slice,
            receive: Receive::ByValue | Receive::ByRef,
            ..
        }) => Ok(RenderedParam {
            decl: indent_statements(&encoded_slice_declarations(accessor, &name)),
            arg: format!("{name}_ptr, {name}_len"),
            cleanup: format!("    RB_ALLOCV_END({name}_tmp);\n"),
        }),
        _ => Err(()),
    }
}

/// Joins C statements into a single block, indented four spaces per line.
fn indent_statements(statements: &[String]) -> String {
    statements
        .iter()
        .map(|statement| format!("    {statement}\n"))
        .collect()
}

fn render_return(
    c_function: &CFunction,
    plan: &ReturnPlan<Native, OutOfRust>,
) -> Result<(String, String)> {
    match plan {
        ReturnPlan::Void => Ok(("CALL;".to_owned(), "Qnil".to_owned())),
        ReturnPlan::DirectViaReturnSlot {
            ty: DirectValueType::Primitive(primitive),
        } => {
            let c_type = c_type_name(c_function.returns())?;
            Ok((
                format!("{c_type} _ffi_ret = CALL;"),
                primitive_box(*primitive),
            ))
        }
        ReturnPlan::DirectViaReturnSlot {
            ty: DirectValueType::Enum(_),
        } => {
            let c_type = c_type_name(c_function.returns())?;
            Ok((
                format!("{c_type} _ffi_ret = CALL;"),
                "LL2NUM((long long)_ffi_ret)".to_owned(),
            ))
        }
        ReturnPlan::EncodedViaReturnSlot {
            ty: TypeRef::String,
            shape: native::BufferShape::Buffer,
            ..
        } => Ok((
            "FfiBuf_u8 _ffi_ret = CALL;".to_owned(),
            "boltffi_ruby_decode_string(_ffi_ret)".to_owned(),
        )),
        ReturnPlan::EncodedViaReturnSlot {
            ty: TypeRef::Bytes,
            shape: native::BufferShape::Buffer,
            ..
        } => Ok((
            "FfiBuf_u8 _ffi_ret = CALL;".to_owned(),
            "boltffi_ruby_decode_bytes(_ffi_ret)".to_owned(),
        )),
        _ => Err(Error::UnsupportedTarget {
            target: "ruby",
            shape: "unsupported return",
        }),
    }
}

fn primitive_declarations(
    accessor: &str,
    name: &str,
    primitive: boltffi_binding::Primitive,
) -> Result<Vec<String>> {
    crate::target::ruby::render::function::primitive_declarations(accessor, name, primitive)
}

fn encoded_slice_declarations(accessor: &str, name: &str) -> Vec<String> {
    crate::target::ruby::render::function::encoded_slice_declarations(accessor, name)
}

fn is_variadic(argc: usize) -> bool {
    crate::target::ruby::render::function::is_variadic(argc)
}

fn arg_accessor(index: usize, variadic: bool) -> String {
    crate::target::ruby::render::function::arg_accessor(index, variadic)
}

fn registration_arity(argc: usize) -> i32 {
    crate::target::ruby::render::function::registration_arity(argc)
}

/// Opening of a class method / initializer wrapper: the `static VALUE` signature
/// plus, for the variadic fallback only, the manual arity check. Fixed-arity
/// wrappers (the common case) let the VM enforce the count, so YJIT can pass
/// arguments in registers and elide the check.
fn wrapper_open(wrapper: &str, argc: usize) -> String {
    let signature = crate::target::ruby::render::function::wrapper_signature_params(argc);
    if is_variadic(argc) {
        format!(
            "static VALUE {wrapper}({signature}) {{\n    if (argc != {argc}) rb_raise(rb_eArgError, \"wrong number of arguments (given %d, expected {argc})\", argc);\n"
        )
    } else {
        format!("static VALUE {wrapper}({signature}) {{\n")
    }
}

fn primitive_box(primitive: boltffi_binding::Primitive) -> String {
    crate::target::ruby::render::function::primitive_box(primitive)
}

fn c_type_name(ty: &Type) -> Result<String> {
    crate::target::ruby::render::function::c_type_name(ty)
}
