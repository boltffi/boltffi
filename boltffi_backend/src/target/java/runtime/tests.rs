use super::Loader;
use crate::target::{
    java::{JavaDesktopLoader, name_style::JavaPackage, syntax::Identifier},
    jvm::{LibraryName, NativeLibraries},
};

#[test]
fn bundled_loader_preserves_android_and_desktop_fallback_order() {
    let libraries = NativeLibraries::boltffi().unwrap();
    let loader = Loader::new(
        Identifier::known("Native"),
        Identifier::known("BoltFFINativeRuntime"),
        &libraries,
    );
    let source = format!(
        "{}\n{}",
        loader.render().unwrap(),
        loader
            .desktop_source(&JavaPackage::parse("com.boltffi.demo").unwrap())
            .unwrap()
            .unwrap()
    );

    assert!(source.contains("BoltFFINativeRuntime.load("));
    assert!(source.contains("System.loadLibrary(\"boltffi\");"));
    assert!(source.contains("import java.io.File;"));
    assert!(source.contains("import java.nio.file.Files;"));
    assert!(source.contains("import java.nio.file.attribute.AclEntry;"));
    assert!(source.contains("import java.util.stream.Collectors;"));
    assert!(!source.contains("java.io.File.createTempFile"));
    assert!(!source.contains("java.nio.file.Files.createDirectory"));
    assert!(source.contains("loadDesktopLibraries("));
    assert!(source.contains("\"boltffi_jni\""));
    assert!(source.contains("failure.addSuppressed(other.failure);"));
    assert!(source.contains("DesktopLibraries.prepare("));
    assert!(source.contains("ExtractionRoot extraction = new ExtractionRoot();"));
    assert!(source.contains("fallbackName,\n                extraction"));
    assert!(source.contains("Files.newOutputStream("));
    assert!(source.contains("StandardOpenOption.CREATE_NEW"));
    assert!(source.contains("Files.createTempDirectory("));
    assert!(source.contains("\"boltffi-native-\","));
    assert!(!source.contains("MessageDigest"));
    assert!(!source.contains("getFD().sync"));
    assert!(!source.contains("Files.move("));
    assert!(!source.contains(".renameTo("));
    assert!(!source.contains("System.getProperty(\"user.name\""));
    assert!(!source.contains("System.getProperty(\"user.home\""));
    assert!(source.contains("security.ownerAttribute(owner)"));
    assert!(source.contains("LinkOption.NOFOLLOW_LINKS"));
    assert!(source.contains("PosixFileAttributes.class"));
    assert!(source.contains("AclFileAttributeView.class"));
    assert!(source.contains("return ACL;"));
    assert!(source.contains("return POSIX;"));
    assert!(source.contains("attributes.owner().equals(expectedOwner)"));
    assert!(source.contains("!attributes.permissions().equals(ownerPermissions())"));
    assert!(source.contains("entry.principal().equals(expectedOwner)"));
    assert!(source.contains("AclEntryFlag.FILE_INHERIT"));
    assert!(source.contains("AclEntryFlag.DIRECTORY_INHERIT"));
    assert!(source.contains("owner-only directory attributes are unavailable"));
    assert!(source.contains("\"boltffi-owner-\","));
    assert!(!source.contains("getUserPrincipalLookupService"));
    assert!(source.contains("owner.getResourceAsStream(resourcePath)"));
    assert!(!source.contains("directory.setWritable"));
    assert!(!source.contains("directory.setReadable"));
    assert!(!source.contains("directory.setExecutable"));
    assert!(!source.contains("BundledLibraryDirectory"));
    assert!(!source.contains("UUID.randomUUID"));
    assert_eq!(source.matches(".deleteOnExit();").count(), 2);
    assert!(!source.contains(".mkdir()"));
    assert!(!source.contains("synchronized"));
    assert!(!source.contains("for ("));

    let android_load = source.find("System.loadLibrary(\"boltffi\");").unwrap();
    let desktop_load = source.find("BoltFFINativeRuntime.load(").unwrap();
    assert!(android_load < desktop_load);

    let preferred_load = source
        .find("LoadResult preferredResult = desktopLibraries.loadPreferred(preferredLibrary);")
        .unwrap();
    let preferred_success = source[preferred_load..]
        .find("if (preferredResult.isLoaded())")
        .map(|offset| preferred_load + offset)
        .unwrap();
    let fallback_preparation = source
        .find("desktopLibraries = desktopLibraries.withFallback(fallbackLibrary);")
        .unwrap();
    assert!(preferred_load < preferred_success);
    assert!(preferred_success < fallback_preparation);

    let fallback_path_start = source
        .find("private LoadResult loadAfterPreferredFailure(")
        .unwrap();
    let load_preferred_start = source[fallback_path_start..]
        .find("private LoadResult loadPreferred(")
        .map(|offset| fallback_path_start + offset)
        .unwrap();
    let fallback_path = &source[fallback_path_start..load_preferred_start];
    let fallback_bundled = fallback_path
        .find("LoadResult fallbackResult = loadFallback(fallbackName);")
        .unwrap();
    let preferred_retry = fallback_path
        .find("return preferredResult.merge(loadPreferred(preferredName));")
        .unwrap();
    assert!(fallback_bundled < preferred_retry);
    assert!(source.contains(
        "LoadResult bundledResult = bundled.tryLoad(libraryName);\n            return bundledResult.isLoaded()"
    ));
    assert!(source.contains("bundledResult.merge(LoadResult.system(libraryName))"));

    let acl = source.find("return ACL;").unwrap();
    let posix = source.find("return POSIX;").unwrap();
    assert!(acl < posix);
}

#[test]
fn portable_native_library_names_render_as_typed_string_literals() {
    let libraries = NativeLibraries::boltffi()
        .unwrap()
        .with_android(LibraryName::parse("android-native").unwrap())
        .with_desktop_jni(LibraryName::parse("jni-$native").unwrap())
        .with_desktop_fallback(LibraryName::parse("fallback-native").unwrap());
    let source = Loader::new(
        Identifier::known("Native"),
        Identifier::known("BoltFFINativeRuntime"),
        &libraries,
    )
    .render()
    .unwrap();

    assert!(source.contains("System.loadLibrary(\"android-native\");"));
    assert!(source.contains("\"jni-$native\","));
    assert!(source.contains("\"fallback-native\""));
}

#[test]
fn system_loader_uses_the_desktop_fallback_without_extraction() {
    let libraries = NativeLibraries::boltffi()
        .unwrap()
        .with_desktop_loader(JavaDesktopLoader::System);
    let source = Loader::new(
        Identifier::known("Native"),
        Identifier::known("BoltFFINativeRuntime"),
        &libraries,
    )
    .render()
    .unwrap();

    assert_eq!(
        source.matches("System.loadLibrary(\"boltffi\");").count(),
        2
    );
    assert!(!source.contains("loadDesktopLibraries"));
    assert!(!source.contains("BundledLibraryDirectory"));
}

#[test]
fn disabled_desktop_loader_retains_android_loading() {
    let libraries = NativeLibraries::boltffi()
        .unwrap()
        .with_desktop_loader(JavaDesktopLoader::None);
    let source = Loader::new(
        Identifier::known("Native"),
        Identifier::known("BoltFFINativeRuntime"),
        &libraries,
    )
    .render()
    .unwrap();

    assert!(source.contains("if (androidRuntime)"));
    assert_eq!(
        source.matches("System.loadLibrary(\"boltffi\");").count(),
        1
    );
    assert!(!source.contains("else {"));
    assert!(!source.contains("loadDesktopLibraries"));
}
