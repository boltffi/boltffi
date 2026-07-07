use std::path::PathBuf;

pub mod render;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::{
        c::CBridge,
        python_cext::{PythonCExtBridge, PythonCExtBridgeContract},
    },
    core::{
        BindingCapability, BridgeCapability, BridgeLayer, CapabilityRequirements, Emitted,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result, Target,
        contract::sealed, host,
    },
    target::python::{
        name_style::{Name, PackageModule},
        render::Package,
    },
};

/// Python host renderer for a CPython extension bridge.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct PythonCExtHost {
    module: Option<PackageModule>,
    distribution: Option<String>,
    version: Option<String>,
    library: Option<String>,
}

impl PythonCExtHost {
    /// Creates a Python host renderer for a CPython extension module.
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects the generated Python package module name.
    pub fn module_name(mut self, module: impl Into<String>) -> Result<Self> {
        self.module = Some(PackageModule::parse(module)?);
        Ok(self)
    }

    /// Selects the generated Python wheel distribution name.
    pub fn distribution_name(mut self, name: impl Into<String>) -> Self {
        self.distribution = Some(name.into());
        self
    }

    /// Selects the generated Python wheel version.
    pub fn version(mut self, version: Option<String>) -> Self {
        self.version = version;
        self
    }

    /// Selects the native shared library artifact loaded by the Python package.
    pub fn native_library(mut self, library: impl Into<String>) -> Self {
        self.library = Some(library.into());
        self
    }

    /// Creates the backend target stack for this Python host.
    pub fn into_target(
        self,
        bindings: &Bindings<Native>,
    ) -> Result<Target<Self, BridgeLayer<CBridge, PythonCExtBridge>>> {
        let source = self.native_module_source_path(bindings)?;
        Ok(Target::new(
            self,
            BridgeLayer::new(
                CBridge::default_header()?,
                PythonCExtBridge::new("_native", source)?,
            ),
        ))
    }

    fn native_module_source_path(&self, bindings: &Bindings<Native>) -> Result<PathBuf> {
        let module = self.package_module(bindings)?;
        Ok(PathBuf::from(module.as_str()).join("_native.c"))
    }
}

impl host::HostBackend for PythonCExtHost {
    type Surface = Native;
    type Bridge = PythonCExtBridgeContract;
    type Syntax = super::Syntax;

    fn name(&self) -> &'static str {
        "python"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Callbacks)
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::PythonExtension)
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Record::from_declaration(decl, bridge, context)?.render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Enumeration::from_declaration(decl, bridge, context)?.render()
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Function::from_declaration(decl, bridge, context)?.render()
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Class::from_declaration(decl, bridge, context)?.render()
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Callback::from_declaration(decl, bridge, context)?.render()
    }

    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Stream::from_declaration(decl, bridge, context)?.render()
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Constant::from_declaration(decl, bridge, context)?.render()
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::primary(""))
    }

    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        let diagnostics = declarations
            .iter()
            .flat_map(|declaration| declaration.emitted().diagnostics().iter().cloned())
            .collect();
        let mut output = GeneratedOutput::combine([
            render::NativeModule::new(bridge, context, declarations).render()?,
            Package::new(
                bindings,
                bridge,
                self.package_module(bindings)?,
                self.resolved_distribution(bindings)?,
                self.package_version(bindings),
                self.native_library_name(bindings)?,
            )
            .render()?,
        ]);
        output.append(GeneratedOutput::new(Vec::new(), diagnostics));
        Ok(output)
    }
}

impl PythonCExtHost {
    fn package_module(&self, bindings: &Bindings<Native>) -> Result<PackageModule> {
        self.module
            .clone()
            .map(Ok)
            .unwrap_or_else(|| PackageModule::from_canonical(bindings.package().name()))
    }

    fn resolved_distribution(&self, bindings: &Bindings<Native>) -> Result<String> {
        self.distribution
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Name::new(bindings.package().name()).function_text())
    }

    fn package_version(&self, bindings: &Bindings<Native>) -> Option<String> {
        self.version
            .clone()
            .or_else(|| bindings.package().version().map(str::to_owned))
    }

    fn native_library_name(&self, bindings: &Bindings<Native>) -> Result<String> {
        self.library
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Name::new(bindings.package().name()).function_text())
    }
}

impl sealed::HostBackend for PythonCExtHost {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{
        bridge::{c::CBridge, python_cext::PythonCExtBridge},
        core::{BridgeLayer, Error, GeneratedOutput, Target},
        target::python::PythonCExtHost,
    };

    fn empty_bindings() -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str("").expect("valid empty source"),
            PackageInfo::new("demo", None),
        )
        .expect("empty source should scan");
        lower::<Native>(&source).expect("empty source should lower")
    }

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn target() -> Target<PythonCExtHost, BridgeLayer<CBridge, PythonCExtBridge>> {
        target_with_host(PythonCExtHost::new())
    }

    fn target_with_host(
        host: PythonCExtHost,
    ) -> Target<PythonCExtHost, BridgeLayer<CBridge, PythonCExtBridge>> {
        Target::new(
            host,
            BridgeLayer::new(
                CBridge::default_header().expect("C header bridge"),
                PythonCExtBridge::native_module().expect("CPython extension bridge"),
            ),
        )
    }

    fn target_with_module(
        module: &str,
    ) -> Target<PythonCExtHost, BridgeLayer<CBridge, PythonCExtBridge>> {
        target_with_host(
            PythonCExtHost::new()
                .module_name(module)
                .expect("Python package module"),
        )
    }

    fn extension(output: &GeneratedOutput) -> &str {
        file(output, "_native.c")
    }

    fn file(output: &GeneratedOutput, path: impl AsRef<Path>) -> &str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == path.as_ref())
            .map(|file| file.contents())
            .expect("generated file")
    }

    #[test]
    fn python_host_places_native_module_source_inside_package_module() {
        let bindings = empty_bindings();

        assert_eq!(
            PythonCExtHost::new().native_module_source_path(&bindings),
            Ok(Path::new("demo/_native.c").to_path_buf())
        );
        assert_eq!(
            PythonCExtHost::new()
                .module_name("demo_api")
                .expect("Python package module")
                .native_module_source_path(&bindings),
            Ok(Path::new("demo_api/_native.c").to_path_buf())
        );
    }

    #[test]
    fn python_target_completes_cpython_module() {
        let output = target()
            .render(&empty_bindings())
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("static PyObject *boltffi_python_initialize_loader"));
        assert!(extension.contains("static PyMethodDef boltffi_python_methods[]"));
        assert!(extension.contains("{\"_initialize_loader\","));
        assert!(extension.contains("static struct PyModuleDef boltffi_python_module"));
        assert!(extension.contains("PyMODINIT_FUNC PyInit__native(void)"));
    }

    #[test]
    fn python_target_renders_primitive_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("static int boltffi_python_parse_i32"));
        assert!(extension.contains("static PyObject *boltffi_python_box_i32"));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_function_demo_add"
        ));
        assert!(extension.contains("if (nargs != 2)"));
        assert!(extension.contains("boltffi_python_parse_i32(args[0], &left)"));
        assert!(extension.contains("boltffi_python_parse_i32(args[1], &right)"));
        assert!(extension.contains(
            "result = boltffi_python_box_i32(boltffi_python_boltffi_function_demo_add(left, right));"
        ));
        assert!(extension.contains("return result;"));
        assert!(extension.contains(
            "{\"add\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_function_demo_add, METH_FASTCALL, NULL}"
        ));
    }

    #[test]
    fn python_target_emits_package_files_for_free_functions() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("Python target should render");
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");
        let setup = file(&output, "setup.py");
        let pyproject = file(&output, "pyproject.toml");

        assert!(pyproject.contains("setuptools>=68"));
        assert!(pyproject.ends_with('\n'));
        assert!(setup.ends_with('\n'));
        assert!(init.ends_with('\n'));
        assert!(stub.ends_with('\n'));
        assert_eq!(file(&output, "demo/py.typed"), "");
        assert!(init.contains("from . import _native"));
        assert!(init.contains("_native._initialize_loader"));
        assert!(init.contains("def add(left: int, right: int) -> int:"));
        assert!(init.contains("return _native.add(left, right)"));
        assert!(init.contains("MODULE_NAME = \"demo\""));
        assert!(init.contains("PACKAGE_NAME = \"demo\""));
        assert!(init.contains("\"add\","));
        assert!(stub.contains("def add(left: int, right: int) -> int: ..."));
        assert!(setup.contains(
            "extension_compile_args = [\"/std:c11\", \"/experimental:c11atomics\"] if sys.platform == \"win32\" else []"
        ));
        assert!(setup.contains("Extension(\n            \"demo._native\","));
        assert!(setup.contains("sources=[\"_native.c\"]"));
        assert!(setup.contains("extra_compile_args=extension_compile_args"));
    }

    #[test]
    fn python_target_keeps_native_extension_name_inside_custom_package_module() {
        let output = target_with_module("demo_api")
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("Python target should render custom module");
        let init = file(&output, "demo_api/__init__.py");
        let stub = file(&output, "demo_api/__init__.pyi");
        let setup = file(&output, "setup.py");

        assert!(file(&output, "_native.c").contains("PyMODINIT_FUNC PyInit__native(void)"));
        assert!(init.contains("from . import _native"));
        assert!(init.contains("return _native.add(left, right)"));
        assert!(init.contains("MODULE_NAME = \"demo_api\""));
        assert!(init.contains("PACKAGE_NAME = \"demo\""));
        assert!(init.contains("return \"libdemo.dylib\""));
        assert!(stub.contains("def add(left: int, right: int) -> int: ..."));
        assert!(setup.contains("packages=[\"demo_api\"]"));
        assert!(setup.contains("Extension(\n            \"demo_api._native\","));
        assert!(setup.contains("sources=[\"_native.c\"]"));
    }

    #[test]
    fn python_target_uses_configured_package_and_library_names() {
        let host = PythonCExtHost::new()
            .module_name("demo_api")
            .expect("Python package module")
            .distribution_name("demo-wheel")
            .version(Some("1.2.3".to_owned()))
            .native_library("demo_ffi");
        let output = target_with_host(host)
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("Python target should render custom package names");
        let init = file(&output, "demo_api/__init__.py");
        let setup = file(&output, "setup.py");

        assert!(init.contains("PACKAGE_NAME = \"demo-wheel\""));
        assert!(init.contains("PACKAGE_VERSION = \"1.2.3\""));
        assert!(init.contains("return \"libdemo_ffi.dylib\""));
        assert!(setup.contains("name=\"demo-wheel\""));
        assert!(setup.contains("version=\"1.2.3\""));
        assert!(setup.contains("packages=[\"demo_api\"]"));
    }

    #[test]
    fn python_target_renders_direct_record_package_and_native_conversions() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[export]
                pub fn echo_point(value: Point) -> Point {
                    value
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("static PyObject *boltffi_python_point_type = NULL;"));
        assert!(extension.contains("static PyObject *boltffi_python_wrapper_register_point"));
        assert!(extension.contains("static int boltffi_python_parse_point"));
        assert!(extension.contains("static PyObject *boltffi_python_box_point"));
        assert!(extension.contains("___Point value;"));
        assert!(extension.contains("boltffi_python_parse_point(args[0], &value)"));
        assert!(extension.contains(
            "result = boltffi_python_box_point(boltffi_python_boltffi_function_demo_echo_point(value));"
        ));
        assert!(extension.contains(
            "{\"_register_point\", (PyCFunction)boltffi_python_wrapper_register_point, METH_FASTCALL, NULL}"
        ));
        assert!(extension.contains("Py_CLEAR(boltffi_python_point_type);"));
        assert!(init.contains("@dataclass(frozen=True, slots=True)\nclass Point:"));
        assert!(init.contains("    x: float"));
        assert!(init.contains("    y: float"));
        assert!(init.contains("_native._register_point(Point)"));
        assert!(stub.contains("class Point:"));
        assert!(stub.contains("def echo_point(value: Point) -> Point: ..."));
    }

    #[test]
    fn python_target_renders_borrowed_direct_record_param_as_addressed_local() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[export]
                pub fn distance(point: &Point) -> f64 {
                    point.x
                }
                "#,
            ))
            .expect("Python target should render borrowed direct record params");
        let extension = extension(&output);
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("___Point point;"));
        assert!(extension.contains("boltffi_python_parse_point(args[0], &point)"));
        assert!(
            extension.contains("boltffi_python_boltffi_function_demo_distance(&point)"),
            "borrowed direct records should stay Python record-shaped and pass an addressed local to the pointer-shaped C ABI\n{extension}"
        );
        assert!(
            !extension.contains("boltffi_python_boltffi_function_demo_distance(point)"),
            "borrowed direct records must not keep passing the local by value after the C ABI becomes pointer-shaped\n{extension}"
        );
        assert!(stub.contains("def distance(point: Point) -> float: ..."));
    }

    #[test]
    fn python_target_renders_direct_record_vector_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[export]
                pub fn echo_points(values: Vec<Point>) -> Vec<Point> {
                    values
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("static int boltffi_python_parse_vec_point"));
        assert!(extension.contains("static PyObject *boltffi_python_decode_owned_vec_point"));
        assert!(extension.contains("___Point *values = NULL;"));
        assert!(extension.contains(
            "boltffi_python_parse_point(PySequence_Fast_GET_ITEM(sequence, index), &values[index])"
        ));
        assert!(extension.contains("item = boltffi_python_box_point(values[index]);"));
        assert!(extension.contains(
            "boltffi_python_parse_vec_point(args[0], &values_wire, &values_ptr, &values_len)"
        ));
        assert!(extension.contains(
            "result = boltffi_python_decode_owned_vec_point(boltffi_python_boltffi_function_demo_echo_points((const ___Point *)values_ptr, values_len));"
        ));
        assert!(stub.contains("from collections.abc import Sequence"));
        assert!(stub.contains("def echo_points(values: Sequence[Point]) -> list[Point]: ..."));
    }

    #[test]
    fn python_target_renders_primitive_vector_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo_numbers(values: Vec<i32>) -> Vec<i32> {
                    values
                }
                "#,
            ))
            .expect("Python target should render primitive vector");
        let extension = extension(&output);
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("static int boltffi_python_parse_vec_i32"));
        assert!(extension.contains("static PyObject *boltffi_python_decode_owned_vec_i32"));
        assert!(extension.contains("int32_t *values = NULL;"));
        assert!(extension.contains(
            "boltffi_python_parse_vec_i32(args[0], &values_wire, &values_ptr, &values_len)"
        ));
        assert!(extension.contains(
            "result = boltffi_python_decode_owned_vec_i32(boltffi_python_boltffi_function_demo_echo_numbers((const int32_t *)values_ptr, values_len));"
        ));
        assert!(stub.contains("from collections.abc import Sequence"));
        assert!(stub.contains("def echo_numbers(values: Sequence[int]) -> list[int]: ..."));
    }

    #[test]
    fn python_target_renders_nested_vector_lengths_from_bound_value() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo_values(values: Vec<Vec<i32>>) -> Vec<Vec<i32>> {
                    values
                }
                "#,
            ))
            .expect("Python target should render nested vector");
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(init.contains(
            "_boltffi_wire_sequence(__boltffi_value_0, len(__boltffi_value_0), lambda __boltffi_value_1: _boltffi_wire_i32(__boltffi_value_1))"
        ));
        assert!(!init.contains("len(self)"));
        assert!(stub.contains("from collections.abc import Sequence"));
        assert!(
            stub.contains(
                "def echo_values(values: Sequence[Sequence[int]]) -> list[list[int]]: ..."
            )
        );
    }

    #[test]
    fn python_target_renders_record_vector_lengths_from_field_value() {
        let output = target()
            .render(&bindings(
                r#"
                #[data]
                pub struct Profile {
                    tags: Vec<String>,
                }

                #[export]
                pub fn echo_profile(profile: Profile) -> Profile {
                    profile
                }
                "#,
            ))
            .expect("Python target should render record vector");
        let init = file(&output, "demo/__init__.py");

        assert!(init.contains(
            "_boltffi_wire_sequence(self.tags, len(self.tags), lambda __boltffi_value_0: _boltffi_wire_string(__boltffi_value_0))"
        ));
        assert!(!init.contains("self.tags.tags"));
    }

    #[test]
    fn python_target_renders_direct_record_associated_callables() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[data(impl)]
                impl Point {
                    pub fn origin() -> Self {
                        Self { x: 0.0, y: 0.0 }
                    }

                    pub fn distance_to_origin(&self) -> f64 {
                        0.0
                    }

                    pub fn midpoint_to(left: Point, right: Point) -> Point {
                        left
                    }

                    pub fn sum_x(left: Point, right: Point) -> f64 {
                        left.x + right.x
                    }
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_init_record_demo_point_origin"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_method_record_demo_point_distance_to_origin"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_init_record_demo_point_midpoint_to"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_method_record_demo_point_sum_x"
        ));
        assert!(extension.contains("boltffi_python_parse_point(args[0], &receiver)"));
        assert!(extension.contains(
            "boltffi_python_boltffi_method_record_demo_point_distance_to_origin(receiver)"
        ));
        assert!(extension.contains(
            "{\"_boltffi_point_origin\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_init_record_demo_point_origin, METH_FASTCALL, NULL}"
        ));
        assert!(extension.contains(
            "{\"_boltffi_point_distance_to_origin\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_method_record_demo_point_distance_to_origin, METH_FASTCALL, NULL}"
        ));
        assert!(init.contains("def origin(cls) -> \"Point\":"));
        assert!(init.contains("return _native._boltffi_point_origin()"));
        assert!(init.contains("def distance_to_origin(self) -> float:"));
        assert!(init.contains("return _native._boltffi_point_distance_to_origin(self)"));
        assert!(init.contains("def midpoint_to(cls, left: Point, right: Point) -> \"Point\":"));
        assert!(init.contains("return _native._boltffi_point_midpoint_to(left, right)"));
        assert!(init.contains("def sum_x(left: Point, right: Point) -> float:"));
        assert!(init.contains("return _native._boltffi_point_sum_x(left, right)"));
        assert!(stub.contains("def origin(cls) -> \"Point\": ..."));
        assert!(stub.contains("def distance_to_origin(self) -> float: ..."));
        assert!(stub.contains("def midpoint_to(cls, left: Point, right: Point) -> \"Point\": ..."));
        assert!(stub.contains("def sum_x(left: Point, right: Point) -> float: ..."));
    }

    #[test]
    fn python_target_renders_c_style_enum_package_and_native_conversions() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(i16)]
                #[data]
                pub enum Status {
                    Active = -3,
                    Inactive = 8,
                    Pending = 13,
                }

                #[export]
                pub fn echo_status(value: Status) -> Status {
                    value
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("typedef struct boltffi_python_c_style_enum_registration"));
        assert!(extension.contains(
            "static boltffi_python_c_style_enum_registration boltffi_python_status_registration"
        ));
        assert!(extension.contains("static PyObject *boltffi_python_wrapper_register_status"));
        assert!(extension.contains("static int boltffi_python_parse_status"));
        assert!(extension.contains("static PyObject *boltffi_python_box_status"));
        assert!(extension.contains("uint8_t bytes[2] = {0};"));
        assert!(extension.contains("static const ___Status boltffi_python_status_member_native_values[3] = {\n    -3,\n    8,\n    13\n};"));
        assert!(extension.contains("boltffi_python_write_u16_le(bytes, (uint16_t)native_value);"));
        assert!(extension.contains("boltffi_python_validate_owned_fixed_buffer(buffer, 2)"));
        assert!(extension.contains("native_value = (___Status)boltffi_python_read_u16_le"));
        assert!(extension.contains("___Status value;"));
        assert!(extension.contains("boltffi_python_parse_status(args[0], &value)"));
        assert!(extension.contains(
            "result = boltffi_python_box_status(boltffi_python_boltffi_function_demo_echo_status(value));"
        ));
        assert!(extension.contains(
            "{\"_register_status\", (PyCFunction)boltffi_python_wrapper_register_status, METH_FASTCALL, NULL}"
        ));
        assert!(extension.contains(
            "boltffi_python_clear_c_style_enum_registration(&boltffi_python_status_registration);"
        ));
        assert!(init.contains("from enum import IntEnum"));
        assert!(init.contains("class Status(IntEnum):"));
        assert!(init.contains("    ACTIVE = -3"));
        assert!(init.contains("    INACTIVE = 8"));
        assert!(init.contains("    PENDING = 13"));
        assert!(init.contains("_native._register_status(Status)"));
        assert!(stub.contains("class Status(IntEnum):"));
        assert!(stub.contains("def echo_status(value: Status) -> Status: ..."));
    }

    #[test]
    fn python_target_renders_c_style_enum_vector_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(i32)]
                #[data]
                pub enum Status {
                    Active = 0,
                    Inactive = 1,
                    Pending = 2,
                }

                #[export]
                pub fn echo_statuses(values: Vec<Status>) -> Vec<Status> {
                    values
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("static int boltffi_python_wire_raw"));
        assert!(extension.contains("static PyObject *boltffi_python_decode_owned_raw_wire"));
        assert!(extension.contains("PyObject *values_wire = NULL;"));
        assert!(extension.contains("const uint8_t *values_ptr = NULL;"));
        assert!(extension.contains("uintptr_t values_len = 0;"));
        assert!(
            extension.contains(
                "boltffi_python_wire_raw(args[0], &values_wire, &values_ptr, &values_len)"
            )
        );
        assert!(extension.contains(
            "result = boltffi_python_decode_owned_raw_wire(boltffi_python_boltffi_function_demo_echo_statuses(values_ptr, values_len));"
        ));
        assert!(init.contains(
            "_boltffi_wire_sequence(values, len(values), lambda __boltffi_value_0: _boltffi_wire_i32(_boltffi_enum_value(__boltffi_value_0, Status, \"Status\")))"
        ));
        assert!(init.contains(
            "_boltffi_read_wire(_native.echo_statuses(_boltffi_wire_sequence(values, len(values), lambda __boltffi_value_0: _boltffi_wire_i32(_boltffi_enum_value(__boltffi_value_0, Status, \"Status\")))), lambda reader: reader.sequence(lambda: Status(reader.i32())))"
        ));
        assert!(stub.contains("from collections.abc import Sequence"));
        assert!(stub.contains("def echo_statuses(values: Sequence[Status]) -> list[Status]: ..."));
    }

    #[test]
    fn python_target_renders_c_style_enum_associated_callables() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(i32)]
                #[data]
                pub enum Direction {
                    North = 0,
                    South = 1,
                }

                #[data(impl)]
                impl Direction {
                    pub fn north() -> Self {
                        Self::North
                    }

                    pub fn opposite(self) -> Self {
                        Self::South
                    }

                    pub fn choose(value: Direction) -> Direction {
                        value
                    }

                    pub fn is_north(value: Direction) -> bool {
                        matches!(value, Self::North)
                    }
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_init_enum_demo_direction_north"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_method_enum_demo_direction_opposite"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_init_enum_demo_direction_choose"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_method_enum_demo_direction_is_north"
        ));
        assert!(extension.contains("boltffi_python_parse_direction(args[0], &receiver)"));
        assert!(
            extension
                .contains("boltffi_python_boltffi_method_enum_demo_direction_opposite(receiver)")
        );
        assert!(extension.contains(
            "{\"_boltffi_direction_north\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_init_enum_demo_direction_north, METH_FASTCALL, NULL}"
        ));
        assert!(extension.contains(
            "{\"_boltffi_direction_opposite\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_method_enum_demo_direction_opposite, METH_FASTCALL, NULL}"
        ));
        assert!(init.contains("class Direction(IntEnum):"));
        assert!(init.contains("def north(cls) -> \"Direction\":"));
        assert!(init.contains("return _native._boltffi_direction_north()"));
        assert!(init.contains("def opposite(self) -> Direction:"));
        assert!(init.contains("return _native._boltffi_direction_opposite(self)"));
        assert!(init.contains("def choose(cls, value: Direction) -> \"Direction\":"));
        assert!(init.contains("return _native._boltffi_direction_choose(value)"));
        assert!(init.contains("def is_north(value: Direction) -> bool:"));
        assert!(init.contains("return _native._boltffi_direction_is_north(value)"));
        assert!(stub.contains("def north(cls) -> \"Direction\": ..."));
        assert!(stub.contains("def opposite(self) -> Direction: ..."));
        assert!(stub.contains("def choose(cls, value: Direction) -> \"Direction\": ..."));
        assert!(stub.contains("def is_north(value: Direction) -> bool: ..."));
    }

    #[test]
    fn python_target_renders_data_enum_payload_parameters() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(i32)]
                #[data]
                pub enum ApiResult {
                    Success = 0,
                    ErrorCode(i32) = 1,
                    ErrorWithValues(Vec<i32>) = 2,
                    ErrorPair((i32, i32)) = 3,
                }

                #[export]
                pub fn is_success(value: ApiResult) -> bool {
                    matches!(value, ApiResult::Success)
                }
                "#,
            ))
            .expect("Python target should render data enum payload parameters");
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(init.contains("class ApiResultErrorCode(ApiResult):"));
        assert!(init.contains("    field_0: int"));
        assert!(init.contains("_boltffi_wire_i32(self.field_0)"));
        assert!(init.contains(
            "_boltffi_wire_sequence(self.field_0, len(self.field_0), lambda __boltffi_value_0: _boltffi_wire_i32(__boltffi_value_0))"
        ));
        assert!(init.contains("class ApiResultErrorPair(ApiResult):"));
        assert!(init.contains("_boltffi_wire_i32(self.field_0[0])"));
        assert!(init.contains("_boltffi_wire_i32(self.field_0[1])"));
        assert!(!init.contains("self.field_0.field_0"));
        assert!(!init.contains("self[0]"));
        assert!(stub.contains("class ApiResultErrorCode(ApiResult):"));
        assert!(stub.contains("def is_success(value: ApiResult) -> bool: ..."));
    }

    #[test]
    fn python_target_renders_class_package_and_native_handle_wrappers() {
        let output = target()
            .render(&bindings(
                r#"
                pub struct Marker {
                    value: u32,
                }

                #[export(single_threaded)]
                impl Marker {
                    pub fn value(&self) -> u32 {
                        self.value
                    }
                }

                pub struct Engine {
                    value: u32,
                }

                #[export(single_threaded)]
                impl Engine {
                    pub fn new(value: u32) -> Self {
                        Self { value }
                    }

                    pub fn value(&self) -> u32 {
                        self.value
                    }

                    pub fn reset(&mut self) {
                        self.value = 0;
                    }

                    pub fn marker(&self) -> Marker {
                        Marker { value: self.value }
                    }

                    pub fn make_marker(value: u32) -> Marker {
                        Marker { value }
                    }
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_release_class_demo_engine"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_init_class_demo_engine_new"
        ));
        assert!(extension.contains("static PyObject *boltffi_python_callable_wrapper_boltffi_method_class_demo_engine_value"));
        assert!(extension.contains("static PyObject *boltffi_python_callable_wrapper_boltffi_method_class_demo_engine_reset"));
        assert!(extension.contains("static PyObject *boltffi_python_callable_wrapper_boltffi_method_class_demo_engine_marker"));
        assert!(extension.contains("static PyObject *boltffi_python_callable_wrapper_boltffi_method_class_demo_engine_make_marker"));
        assert!(extension.contains("boltffi_python_parse_u64(args[0], &receiver)"));
        assert!(
            extension.contains("boltffi_python_boltffi_method_class_demo_engine_value(receiver)")
        );
        assert!(
            extension.contains("boltffi_python_boltffi_method_class_demo_engine_reset(receiver)")
        );
        assert!(extension.contains(
            "{\"_boltffi_engine_release\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_release_class_demo_engine, METH_FASTCALL, NULL}"
        ));
        assert!(extension.contains(
            "{\"_boltffi_engine_new\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_init_class_demo_engine_new, METH_FASTCALL, NULL}"
        ));
        assert!(init.contains("class Engine:"));
        assert!(init.contains("__slots__ = (\"_handle\",)"));
        assert!(init.contains("def __init__(self, value: int) -> None:"));
        assert!(init.contains("self._handle = _native._boltffi_engine_new(value)"));
        assert!(init.contains("def __del__(self) -> None:"));
        assert!(init.contains("_native._boltffi_engine_release(handle)"));
        assert!(init.contains("def value(self) -> int:"));
        assert!(init.contains("return _native._boltffi_engine_value(self._handle)"));
        assert!(init.contains("def reset(self) -> None:"));
        assert!(init.contains("_native._boltffi_engine_reset(self._handle)"));
        assert!(init.contains("def marker(self) -> Marker:"));
        assert!(
            init.contains(
                "return Marker._from_handle(_native._boltffi_engine_marker(self._handle))"
            )
        );
        assert!(init.contains("def make_marker(value: int) -> Marker:"));
        assert!(
            init.contains("return Marker._from_handle(_native._boltffi_engine_make_marker(value))")
        );
        assert!(stub.contains("class Engine:"));
        assert!(stub.contains("_handle: int"));
        assert!(stub.contains("def __init__(self, value: int) -> None: ..."));
        assert!(stub.contains("def value(self) -> int: ..."));
        assert!(stub.contains("def reset(self) -> None: ..."));
        assert!(stub.contains("def marker(self) -> Marker: ..."));
        assert!(stub.contains("def make_marker(value: int) -> Marker: ..."));
    }

    #[test]
    fn python_target_preserves_rust_pascal_type_spelling() {
        let output = target()
            .render(&bindings(
                r#"
                pub struct GPSSimulator {
                    enabled: bool,
                }

                #[export]
                impl GPSSimulator {
                    pub fn new(enabled: bool) -> Self {
                        Self { enabled }
                    }

                    pub fn enabled(&self) -> bool {
                        self.enabled
                    }
                }
                "#,
            ))
            .expect("Python target should render");
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(init.contains("class GPSSimulator:"));
        assert!(stub.contains("class GPSSimulator:"));
        assert!(!init.contains("class GpsSimulator:"));
        assert!(!stub.contains("class GpsSimulator:"));
    }

    #[test]
    fn python_target_renders_primitive_callback_handles() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub trait ValueCallback {
                    fn on_value(&self, value: i32) -> i32;
                }

                #[export]
                pub fn invoke_value_callback(callback: impl ValueCallback, input: i32) -> i32 {
                    callback.on_value(input)
                }
                "#,
            ))
            .expect("Python target should render callback handles");
        let extension = extension(&output);
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains(
            "static ___ValueCallbackVTable boltffi_python_callback_value_callback_vtable"
        ));
        assert!(extension.contains(".free = boltffi_python_callback_value_callback_free"));
        assert!(extension.contains(".clone = boltffi_python_callback_value_callback_clone"));
        assert!(extension.contains(".on_value = boltffi_python_callback_value_callback_on_value"));
        assert!(extension.contains("PyObject_HasAttrString(result, \"__await__\")"));
        assert!(extension.contains(
            "on_value() returned an awaitable, Python callback methods must return the value directly"
        ));
        assert!(extension.contains("static int boltffi_python_bind_callback_value_callback"));
        assert!(extension.contains("boltffi_python_boltffi_register_callback_demo_value_callback(&boltffi_python_callback_value_callback_vtable)"));
        assert!(extension.contains("static int boltffi_python_parse_callback_value_callback"));
        assert!(extension.contains(
            "boltffi_python_boltffi_create_callback_demo_value_callback((uint64_t)(uintptr_t)value)"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_function_demo_invoke_value_callback"
        ));
        assert!(
            extension.contains("boltffi_python_parse_callback_value_callback(args[0], &callback)")
        );
        assert!(extension.contains(
            "result = boltffi_python_box_i32(boltffi_python_boltffi_function_demo_invoke_value_callback(callback, input));"
        ));
        assert!(extension.contains(
            "{\"invoke_value_callback\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_function_demo_invoke_value_callback"
        ));
        assert!(
            stub.contains("def invoke_value_callback(callback: object, input: int) -> int: ...")
        );
    }

    #[test]
    fn python_target_renders_callback_string_payloads_through_codec() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub trait TextCallback {
                    fn on_text(&self, value: String) -> String;
                }

                #[export]
                pub fn invoke_text_callback(callback: impl TextCallback, value: String) -> String {
                    callback.on_text(value)
                }
                "#,
            ))
            .expect("Python target should render encoded callback payloads");
        let extension = extension(&output);
        let package = file(&output, "demo/__init__.py");

        assert!(extension.contains("value_object = boltffi_python_decode_read_"));
        assert!(extension.contains("if (!boltffi_python_encode_write_"));
        assert!(
            package.contains("return _boltffi_read_wire(data, lambda reader: reader.string())")
        );
        assert!(package.contains("return _boltffi_wire_string("));
        assert!(package.contains("_native._register_wire_codec(\"read_"));
        assert!(package.contains("_native._register_wire_codec(\"write_"));
    }

    #[test]
    fn python_target_renders_closure_string_payloads_through_codec() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn invoke_text_closure(callback: impl Fn(String) -> String, value: String) -> String {
                    callback(value)
                }
                "#,
            ))
            .expect("Python target should render encoded closure payloads");
        let extension = extension(&output);
        let package = file(&output, "demo/__init__.py");

        assert!(extension.contains("boltffi_python_decode_read_"));
        assert!(extension.contains("if (!boltffi_python_encode_write_"));
        assert!(
            package.contains("return _boltffi_read_wire(data, lambda reader: reader.string())")
        );
        assert!(package.contains("return _boltffi_wire_string("));
    }

    #[test]
    fn python_target_renders_callback_sequence_payloads_through_codec() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub trait TextCallback {
                    fn on_text(&self, values: Vec<String>) -> Vec<String>;
                }

                #[export]
                pub fn invoke_text_callback(callback: impl TextCallback, values: Vec<String>) -> Vec<String> {
                    callback.on_text(values)
                }
                "#,
            ))
            .expect("Python target should render composite callback payloads");
        let extension = extension(&output);
        let package = file(&output, "demo/__init__.py");

        assert!(extension.contains("values_object = boltffi_python_decode_read_"));
        assert!(extension.contains("if (!boltffi_python_encode_write_"));
        assert!(package.contains("reader.sequence(lambda: reader.string())"));
        assert!(package.contains("_boltffi_wire_sequence("));
    }

    #[test]
    fn python_target_renders_closure_sequence_payloads_through_codec() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn invoke_text_closure(callback: impl Fn(Vec<String>) -> Vec<String>, values: Vec<String>) -> Vec<String> {
                    callback(values)
                }
                "#,
            ))
            .expect("Python target should render composite closure payloads");
        let extension = extension(&output);
        let package = file(&output, "demo/__init__.py");

        assert!(extension.contains("boltffi_python_decode_read_"));
        assert!(extension.contains("if (!boltffi_python_encode_write_"));
        assert!(package.contains("reader.sequence(lambda: reader.string())"));
        assert!(package.contains("_boltffi_wire_sequence("));
    }

    #[test]
    fn python_target_renders_class_stream_subscription() {
        let output = target()
            .render(&bindings(
                r#"
                use std::sync::Arc;
                use boltffi::EventSubscription;

                pub struct EventBus;

                #[export(single_threaded)]
                impl EventBus {
                    pub fn new() -> Self {
                        Self
                    }

                    #[ffi_stream(item = i32)]
                    pub fn subscribe_values(&self) -> Arc<EventSubscription<i32>> {
                        todo!()
                    }
                }
                "#,
            ))
            .expect("Python target should render stream subscription");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains(
            "static PyObject *boltffi_python_stream_wrapper_boltffi_stream_demo_event_bus_subscribe_values_subscribe"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_stream_wrapper_boltffi_stream_demo_event_bus_subscribe_values_pop_batch"
        ));
        assert!(extension.contains("boltffi_python_parse_usize(args[1], &output_capacity)"));
        assert!(extension.contains("int32_t *items = NULL;"));
        assert!(extension.contains("boltffi_python_box_i32(items[item_index])"));
        assert!(extension.contains(
            "{\"subscribe_values\", (PyCFunction)boltffi_python_stream_wrapper_boltffi_stream_demo_event_bus_subscribe_values_subscribe, METH_FASTCALL, NULL}"
        ));
        assert!(
            init.contains("def subscribe_values(self) -> \"EventBusSubscribeValuesSubscription\":")
        );
        assert!(init.contains(
            "return EventBusSubscribeValuesSubscription._from_handle(_native.subscribe_values(self._handle))"
        ));
        assert!(init.contains("class EventBusSubscribeValuesSubscription:"));
        assert!(init.contains("def pop_batch(self, max_count: int = 16) -> list[int]:"));
        assert!(init.contains(
            "return _native.subscribe_values_pop_batch(self._require_handle(), max_count)"
        ));
        assert!(stub.contains(
            "def subscribe_values(self) -> \"EventBusSubscribeValuesSubscription\": ..."
        ));
        assert!(stub.contains("def pop_batch(self, max_count: int = 16) -> list[int]: ..."));
    }

    #[test]
    fn python_target_escapes_c_parameter_identifiers() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo(r#int: i32) -> i32 {
                    r#int
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("int32_t int_;"));
        assert!(extension.contains("boltffi_python_parse_i32(args[0], &int_)"));
        assert!(extension.contains("boltffi_python_boltffi_function_demo_echo(int_)"));
    }

    #[test]
    fn python_target_escapes_python_method_keywords() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn lambda() -> i32 {
                    1
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains(
            "{\"lambda_\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_function_demo_lambda, METH_FASTCALL, NULL}"
        ));
    }

    #[test]
    fn python_target_renders_inline_constants_in_package() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(i32)]
                #[data]
                pub enum Mode {
                    Fast = 1,
                    Slow = 2,
                }

                #[export]
                pub const ANSWER: u32 = 42;

                #[export]
                pub const NAME: &'static str = "bolt";

                #[export]
                pub const DEFAULT_MODE: Mode = Mode::Fast;
                "#,
            ))
            .expect("Python target should render constants");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(!extension.contains("boltffi_python_callable_wrapper_boltffi_const_demo_answer"));
        assert!(init.contains("answer: int = 42"));
        assert!(init.contains("name: str = \"bolt\""));
        assert!(init.contains("default_mode: Mode = Mode.FAST"));
        assert!(init.contains("\"answer\","));
        assert!(init.contains("\"name\","));
        assert!(init.contains("\"default_mode\","));
        assert!(stub.contains("answer: int"));
        assert!(stub.contains("name: str"));
        assert!(stub.contains("default_mode: Mode"));
    }

    #[test]
    fn python_target_renders_record_field_defaults() {
        let output = target()
            .render(&bindings(
                r#"
                #[data]
                pub struct Config {
                    pub name: String,
                    #[boltffi::default(3)]
                    pub retries: i32,
                    #[boltffi::default(true)]
                    pub enabled: bool,
                    #[boltffi::default("localhost")]
                    pub host: String,
                }
                "#,
            ))
            .expect("Python target should render record defaults");
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(init.contains("name: str"));
        assert!(init.contains("retries: int = 3"));
        assert!(init.contains("enabled: bool = True"));
        assert!(init.contains("host: str = \"localhost\""));
        assert!(stub.contains("retries: int = 3"));
        assert!(stub.contains("enabled: bool = True"));
        assert!(stub.contains("host: str = \"localhost\""));
    }

    #[test]
    fn python_target_renders_uuid_as_python_uuid() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo(value: uuid::Uuid) -> uuid::Uuid {
                    value
                }
                "#,
            ))
            .expect("Python target should render UUID builtins");
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(init.contains("import uuid"));
        assert!(init.contains("def _boltffi_wire_uuid(value: uuid.UUID | str) -> bytes:"));
        assert!(init.contains("def echo(value: uuid.UUID) -> uuid.UUID:"));
        assert!(init.contains("return uuid.UUID(bytes=high + low)"));
        assert!(stub.contains("import uuid"));
        assert!(stub.contains("def echo(value: uuid.UUID) -> uuid.UUID: ..."));
    }

    #[test]
    fn python_target_renders_accessor_constants_through_native_module() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub const MAGIC: &'static [u8] = b"ffi";
                "#,
            ))
            .expect("Python target should render accessor constant");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("static PyObject *boltffi_python_decode_owned_raw_wire"));
        assert!(
            extension.contains(
                "static PyObject *boltffi_python_callable_wrapper_boltffi_const_demo_magic"
            )
        );
        assert!(extension.contains(
            "result = boltffi_python_decode_owned_raw_wire(boltffi_python_boltffi_const_demo_magic());"
        ));
        assert!(extension.contains(
            "{\"magic\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_const_demo_magic, METH_FASTCALL, NULL}"
        ));
        assert!(init.contains(
            "magic: bytes = _boltffi_read_wire(_native.magic(), lambda reader: reader.bytes())"
        ));
        assert!(init.contains("\"magic\","));
        assert!(stub.contains("magic: bytes"));
    }

    #[test]
    fn python_target_renders_async_functions_through_native_future_protocol() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub async fn fetch() -> i32 {
                    1
                }
                "#,
            ))
            .expect("Python target should render async functions");
        let header = file(&output, "boltffi.h");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(header.contains("RustFutureHandle boltffi_function_demo_fetch(void);"));
        assert!(header.contains("void boltffi_async_function_demo_fetch_poll("));
        assert!(header.contains("int32_t boltffi_async_function_demo_fetch_complete("));
        assert!(extension.contains("static PyObject *boltffi_python_box_future_handle"));
        assert!(extension.contains("static void boltffi_python_future_wake"));
        assert!(extension.contains(
            "{\"fetch__complete\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_async_function_demo_fetch_complete, METH_O, NULL}"
        ));
        assert!(init.contains("import asyncio"));
        assert!(init.contains("class _BoltFfiNativeFuture:"));
        assert!(init.contains("async def fetch() -> int:"));
        assert!(init.contains("__boltffi_future = _BoltFfiNativeFuture("));
        assert!(init.contains("return await __boltffi_future.wait()"));
        assert!(stub.contains("async def fetch() -> int: ..."));
    }

    #[test]
    fn python_target_decodes_async_result_error_payloads() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub async fn fetch(value: i32) -> Result<i32, String> {
                    if value > 0 {
                        Ok(value)
                    } else {
                        Err("invalid value".to_string())
                    }
                }
                "#,
            ))
            .expect("Python target should render async result errors");
        let init = file(&output, "demo/__init__.py");

        assert!(init.contains("except RuntimeError as error:"));
        assert!(init.contains("raise _boltffi_error_exception(decoder(error.args[0])) from error"));
        assert!(init.contains("error_decoder=_boltffi_read_"));
    }

    #[test]
    fn python_target_decodes_sync_result_error_payloads() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn divide(value: i32) -> Result<i32, String> {
                    if value > 0 {
                        Ok(value)
                    } else {
                        Err("invalid value".to_string())
                    }
                }
                "#,
            ))
            .expect("Python target should render sync result errors");
        let init = file(&output, "demo/__init__.py");

        assert!(init.contains("def _boltffi_call(error_decoder, call):"));
        assert!(
            init.contains(
                "raise _boltffi_error_exception(error_decoder(error.args[0])) from error"
            )
        );
        assert!(init.contains("return _boltffi_call(_boltffi_read_"));
    }

    #[test]
    fn python_target_renders_typed_result_exceptions() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(i32)]
                #[data]
                pub enum ParseError {
                    Empty = 1,
                }

                #[export]
                pub fn parse(value: String) -> Result<i32, ParseError> {
                    if value.is_empty() {
                        Err(ParseError::Empty)
                    } else {
                        Ok(1)
                    }
                }
                "#,
            ))
            .expect("Python target should render typed result errors");
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(init.contains("class ParseErrorException(RuntimeError):"));
        assert!(init.contains("def __init__(self, error: ParseError) -> None:"));
        assert!(init.contains("\"ParseErrorException\","));
        assert!(stub.contains("class ParseErrorException(RuntimeError):"));
        assert!(stub.contains("error: ParseError"));
    }

    #[test]
    fn python_target_requires_bool_objects_for_bool_parameters() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo(flag: bool) -> bool {
                    flag
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("if (!PyBool_Check(value))"));
        assert!(extension.contains("PyErr_SetString(PyExc_TypeError, \"expected bool\")"));
        assert!(extension.contains("*out = value == Py_True;"));
    }

    #[test]
    fn python_target_rejects_records_that_shadow_int_enum_base() {
        let error = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct IntEnum {
                    pub value: i32,
                }

                #[repr(i32)]
                #[data]
                pub enum Status {
                    Ready = 0,
                }
                "#,
            ))
            .expect_err("Python name collision should reject");

        assert!(matches!(
            error,
            Error::PythonNameCollision {
                scope,
                name,
                existing,
                colliding,
            } if scope == "python module"
                && name == "IntEnum"
                && existing == "imported enum base `IntEnum`"
                && colliding == "record `IntEnum`"
        ));
    }

    #[test]
    fn python_target_rejects_record_field_method_name_collisions() {
        let error = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub origin: i32,
                }

                #[data(impl)]
                impl Point {
                    pub fn origin(&self) -> i32 {
                        self.origin
                    }
                }
                "#,
            ))
            .expect_err("Python record member collision should reject");

        assert!(matches!(
            error,
            Error::PythonNameCollision {
                scope,
                name,
                existing,
                colliding,
            } if scope == "record `Point`"
                && name == "origin"
                && existing == "field `origin`"
                && colliding == "method `origin`"
        ));
    }

    #[test]
    fn python_target_rejects_int_enum_reserved_method_names() {
        let error = target()
            .render(&bindings(
                r#"
                #[repr(i32)]
                #[data]
                pub enum Status {
                    Ready = 0,
                }

                #[data(impl)]
                impl Status {
                    pub fn name(&self) -> i32 {
                        0
                    }
                }
                "#,
            ))
            .expect_err("Python enum member collision should reject");

        assert!(matches!(
            error,
            Error::PythonNameCollision {
                scope,
                name,
                existing,
                colliding,
            } if scope == "enum `Status`"
                && name == "name"
                && existing == "reserved IntEnum property `name`"
                && colliding == "method `name`"
        ));
    }

    #[test]
    fn python_target_renders_string_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn greet(name: String) -> String {
                    name
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");

        assert!(extension.contains("static int boltffi_python_wire_raw"));
        assert!(extension.contains("static PyObject *boltffi_python_decode_owned_raw_wire"));
        assert!(extension.contains("PyObject *name_wire = NULL;"));
        assert!(extension.contains("const uint8_t *name_ptr = NULL;"));
        assert!(extension.contains("uintptr_t name_len = 0;"));
        assert!(
            extension
                .contains("boltffi_python_wire_raw(args[0], &name_wire, &name_ptr, &name_len)")
        );
        assert!(extension.contains(
            "result = boltffi_python_decode_owned_raw_wire(boltffi_python_boltffi_function_demo_greet(name_ptr, name_len));"
        ));
        assert!(extension.contains("Py_XDECREF(name_wire);"));
        assert!(extension.contains("boltffi_python_boltffi_free_buf(buffer);"));
        assert!(init.contains("_native.greet(_boltffi_wire_string(name))"));
        assert!(init.contains("lambda reader: reader.string()"));
    }

    #[test]
    fn python_target_renders_bytes_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo(bytes: Vec<u8>) -> Vec<u8> {
                    bytes
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");

        assert!(extension.contains("static int boltffi_python_wire_raw"));
        assert!(extension.contains("static PyObject *boltffi_python_decode_owned_raw_wire"));
        assert!(extension.contains("PyObject *bytes_wire = NULL;"));
        assert!(extension.contains("const uint8_t *bytes_ptr = NULL;"));
        assert!(extension.contains("uintptr_t bytes_len = 0;"));
        assert!(
            extension
                .contains("boltffi_python_wire_raw(args[0], &bytes_wire, &bytes_ptr, &bytes_len)")
        );
        assert!(extension.contains(
            "result = boltffi_python_decode_owned_raw_wire(boltffi_python_boltffi_function_demo_echo(bytes_ptr, bytes_len));"
        ));
        assert!(extension.contains("Py_XDECREF(bytes_wire);"));
        assert!(init.contains("_native.echo(_boltffi_wire_bytes(bytes))"));
        assert!(init.contains("lambda reader: reader.bytes()"));
    }
}
