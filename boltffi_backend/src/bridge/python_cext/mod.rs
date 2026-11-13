//! CPython C extension bridge.
//!
//! This bridge layers above the C ABI bridge. It emits the CPython dynamic
//! loader fragment and typed extension declarations consumed by a Python host
//! renderer.

mod contract;
mod extension;
mod template;

pub use crate::bridge::c::HeaderInclude as CHeaderInclude;
pub use contract::{
    ExtensionMethod, LoadedFunction, MethodFlags, MethodName, ModuleSymbols,
    PythonCExtBridgeContract, PythonExtensionName,
};
pub use extension::PythonCExtBridge;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Native, lower};

    use crate::{
        bridge::{
            c::CBridge,
            python_cext::{MethodFlags, PythonCExtBridge, PythonCExtBridgeContract},
        },
        core::{BridgeLayer, BridgeOutput, BridgeStack},
    };

    fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
        let file = syn::parse_str(source).expect("valid source fixture");
        let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
            .expect("fixture should scan");
        lower::<Native>(&source).expect("fixture should lower")
    }

    fn bridge(source: &str) -> BridgeOutput<PythonCExtBridgeContract> {
        let bindings = bindings(source);
        let stack = BridgeLayer::new(
            CBridge::default_header().expect("C header bridge"),
            PythonCExtBridge::native_module().expect("CPython extension bridge"),
        );
        stack.build(&bindings).expect("Python C extension stack")
    }

    fn bridge_with_paths(
        source: &str,
        header: &str,
        extension: &str,
    ) -> BridgeOutput<PythonCExtBridgeContract> {
        let bindings = bindings(source);
        let stack = BridgeLayer::new(
            CBridge::new(header).expect("C header bridge"),
            PythonCExtBridge::new("_native", extension).expect("CPython extension bridge"),
        );
        stack.build(&bindings).expect("Python C extension stack")
    }

    fn files(source: &str) -> Vec<(String, String)> {
        bridge(source)
            .output()
            .files()
            .iter()
            .map(|file| {
                (
                    file.path().as_path().display().to_string(),
                    file.contents().to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn python_cext_bridge_layers_loader_fragment_on_c_bridge() {
        let output = bridge(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        );
        let files = output
            .output()
            .files()
            .iter()
            .map(|file| {
                (
                    file.path().as_path().display().to_string(),
                    file.contents().to_owned(),
                )
            })
            .collect::<Vec<_>>();

        let header = files
            .iter()
            .find(|(path, _)| path == "boltffi.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let extension = files
            .iter()
            .find(|(path, _)| path == "_native.c")
            .map(|(_, contents)| contents)
            .expect("CPython extension file");

        assert!(
            header.contains("int32_t boltffi_function_4demo3add(int32_t left, int32_t right);")
        );
        assert!(extension.contains("#include \"boltffi.h\""));
        assert!(extension.contains("boltffi_python_boltffi_free_buf"));
        assert!(extension.contains(
            "typedef int32_t (*boltffi_python_boltffi_function_4demo3add_fn)(int32_t, int32_t);"
        ));
        assert!(extension.contains("boltffi_python_boltffi_function_4demo3add = (boltffi_python_boltffi_function_4demo3add_fn)dlsym(boltffi_python_library_handle, \"boltffi_function_4demo3add\");"));
        assert!(extension.contains("static PyObject *boltffi_python_initialize_loader"));
        assert!(extension.contains("static void boltffi_python_free_module"));
        assert!(!extension.contains("static PyMethodDef"));
        assert!(!extension.contains("PyMODINIT_FUNC"));

        let contract = output.contract();
        assert_eq!(contract.module().as_str(), "_native");
        assert_eq!(
            contract.symbols().init_function().as_str(),
            "PyInit__native"
        );
        assert_eq!(
            contract.symbols().method_table().as_str(),
            "boltffi_python_methods"
        );
        assert_eq!(
            contract.loader_method().python_name().as_str(),
            "_initialize_loader"
        );
        assert_eq!(
            contract.loader_method().c_function().as_str(),
            "boltffi_python_initialize_loader"
        );
        assert_eq!(contract.loader_method().flags(), MethodFlags::OneObject);
    }

    #[test]
    fn python_cext_bridge_binds_callback_registration_symbols() {
        let files = files(
            r#"
            #[export]
            pub trait Listener {
                fn on_value(&self, value: i32);
            }
            "#,
        );

        let extension = files
            .iter()
            .find(|(path, _)| path == "_native.c")
            .map(|(_, contents)| contents)
            .expect("CPython extension file");

        assert!(extension.contains("boltffi_create_callback_4demo8Listener"));
        assert!(extension.contains("boltffi_register_callback_4demo8Listener"));
    }

    #[test]
    fn python_cext_bridge_includes_selected_c_header() {
        let output = bridge_with_paths(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
            "include/my_abi.h",
            "_native.c",
        );
        let extension = output
            .output()
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new("_native.c"))
            .map(|file| file.contents())
            .expect("CPython extension file");

        assert!(extension.contains("#include \"include/my_abi.h\""));
        assert!(!extension.contains("#include \"boltffi.h\""));
        assert_eq!(output.contract().c_header().as_str(), "include/my_abi.h");
    }

    #[test]
    fn python_cext_bridge_includes_header_relative_to_nested_extension() {
        let output = bridge_with_paths(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
            "pkg/boltffi.h",
            "pkg/_native.c",
        );
        let extension = output
            .output()
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new("pkg/_native.c"))
            .map(|file| file.contents())
            .expect("CPython extension file");

        assert!(extension.contains("#include \"boltffi.h\""));
        assert_eq!(output.contract().c_header().as_str(), "boltffi.h");
    }
}
