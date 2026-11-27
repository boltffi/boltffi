use std::path::Path;

use boltffi_ast::PackageInfo;
use boltffi_test_source::scan_source;

fn record(name: &str) -> String {
    format!("#[data] pub struct {name} {{ pub value: u32 }}\n")
}

fn write(root: &Path, file: &str, text: String) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().expect("file has a directory"))
        .expect("create source directory");
    std::fs::write(path, text).expect("write source file");
}

#[test]
fn module_files_are_found_where_rustc_looks() {
    let root = std::env::temp_dir().join(format!(
        "boltffi-test-source-module-files-{}",
        std::process::id()
    ));
    write(
        &root,
        "src/lib.rs",
        "pub mod flat; pub mod nested; #[path = \"shapes.rs\"] pub mod geometry;".to_owned(),
    );
    write(
        &root,
        "src/flat.rs",
        record("Flat")
            + "pub mod child;
               #[path = \"beside.rs\"] pub mod renamed;
               pub mod inline { #[path = \"other.rs\"] pub mod inner; }
               #[path = \"moved\"] pub mod relocated { pub mod inner; }",
    );
    write(&root, "src/flat/child.rs", record("FlatChild"));
    write(&root, "src/beside.rs", record("Beside"));
    write(&root, "src/flat/inline/other.rs", record("InlineOther"));
    write(&root, "src/moved/inner.rs", record("Relocated"));
    write(
        &root,
        "src/nested/mod.rs",
        record("Nested") + "pub mod child;",
    );
    write(&root, "src/nested/child.rs", record("NestedChild"));
    write(&root, "src/shapes.rs", record("Shapes") + "pub mod child;");
    write(&root, "src/child.rs", record("ShapesChild"));

    let contract = scan_source(&root.join("src/lib.rs"), PackageInfo::new("demo", None));
    std::fs::remove_dir_all(&root).expect("remove source directory");

    let mut names = contract
        .expect("every module file is found")
        .records
        .iter()
        .map(|record| record.name.spelling().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            "Beside",
            "Flat",
            "FlatChild",
            "InlineOther",
            "Nested",
            "NestedChild",
            "Relocated",
            "Shapes",
            "ShapesChild",
        ]
    );
}
