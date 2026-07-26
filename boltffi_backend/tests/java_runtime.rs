use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use boltffi_ast::PackageInfo;
use boltffi_backend::{CoverageMode, target::java::JavaHost};
use boltffi_binding::{Native, lower};

mod java_toolchain;

use java_toolchain::JavaCompiler;

const LIBRARY_NAME: &str = "boltffi_java_runtime_test";
const PREFERRED_LIBRARY_NAME: &str = "boltffi_java_runtime_preferred";
const FALLBACK_LIBRARY_NAME: &str = "boltffi_java_runtime_fallback";
const CORRUPT_LIBRARY: &[u8] = b"not a native library";
const UNSAFE_CACHE: &[u8] = b"unsafe deterministic cache";
const UNSAFE_CACHE_ENTRY: &str = "occupied";
const SYMLINK_CACHE_TARGET: &str = "attacker-cache";
const JAVA_PACKAGE: &str = "com.boltffi.𐐀runtime";
static NEXT_RUNTIME_FIXTURE: AtomicU64 = AtomicU64::new(0);
const RUST_SOURCE: &str = r#"
    #[repr(C)]
    #[data]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    #[export]
    pub fn round_trip_point(value: Point) -> Point { value }

    #[data]
    pub struct EncodedPoint {
        pub x: f64,
        pub y: f64,
    }

    #[data]
    pub struct NestedIdentity {
        pub maybe_values: Option<Vec<i32>>,
        pub groups: Vec<Vec<u8>>,
        pub choices: Vec<Option<Vec<i16>>>,
    }

    #[data]
    pub struct ResultHolder {
        pub outcome: Result<Vec<i32>, Option<Vec<i16>>>,
    }

    #[data(impl)]
    impl EncodedPoint {
        pub fn new(x: f64, y: f64) -> Self { Self { x, y } }

        pub fn sum(&self) -> f64 { self.x + self.y }

        pub fn scale(&mut self, factor: f64) {
            self.x *= factor;
            self.y *= factor;
        }
    }

    #[export]
    pub fn round_trip_encoded_point(value: EncodedPoint) -> EncodedPoint { value }

    #[export]
    pub fn round_trip_bool(value: bool) -> bool { value }

    #[export]
    pub fn round_trip_i8(value: i8) -> i8 { value }

    #[export]
    pub fn round_trip_u8(value: u8) -> u8 { value }

    #[export]
    pub fn round_trip_i16(value: i16) -> i16 { value }

    #[export]
    pub fn round_trip_u16(value: u16) -> u16 { value }

    #[export]
    pub fn round_trip_i32(value: i32) -> i32 { value }

    #[export]
    pub fn round_trip_u32(value: u32) -> u32 { value }

    #[export]
    pub fn round_trip_i64(value: i64) -> i64 { value }

    #[export]
    pub fn round_trip_u64(value: u64) -> u64 { value }

    #[export]
    pub fn round_trip_isize(value: isize) -> isize { value }

    #[export]
    pub fn round_trip_usize(value: usize) -> usize { value }

    #[export]
    pub fn round_trip_f32(value: f32) -> f32 { value }

    #[export]
    pub fn round_trip_f64(value: f64) -> f64 { value }

    #[export]
    pub fn observe_void() {}

    #[export]
    pub fn observed_void_calls() -> u32 { 0 }
"#;
const C_RUNTIME: &str = r#"
#include "native/boltffi.h"
#include <stdlib.h>
#include <string.h>

void boltffi_free_string(FfiString string) {
    (void)string;
}

void boltffi_free_buf(FfiBuf_u8 buffer) {
    free(buffer.ptr);
}

FfiBuf_u8 boltffi_buf_from_bytes(const uint8_t *pointer, uintptr_t length) {
    uint8_t *bytes = length == 0 ? NULL : malloc(length);
    if (length != 0) {
        memcpy(bytes, pointer, length);
    }
    return (FfiBuf_u8){bytes, length, length, 1};
}

FfiBuf_u8 boltffi_buf_with_len(uintptr_t length) {
    uint8_t *bytes = length == 0 ? NULL : calloc(length, 1);
    return (FfiBuf_u8){bytes, length, length, 1};
}

FfiStatus boltffi_last_error_message(FfiString *output) {
    if (output != NULL) {
        *output = (FfiString){NULL, 0, 0};
    }
    return FFI_STATUS_OK;
}

void boltffi_clear_last_error(void) {}

___Point boltffi_function_demo_round_trip_point(___Point value) {
    return value;
}

static uint64_t read_u64_le(const uint8_t *bytes) {
    uint64_t value = 0;
    uint32_t index = 0;
    while (index < 8) {
        value |= ((uint64_t)bytes[index]) << (index * 8);
        index += 1;
    }
    return value;
}

static double read_f64_le(const uint8_t *bytes) {
    uint64_t bits = read_u64_le(bytes);
    double value;
    memcpy(&value, &bits, sizeof(value));
    return value;
}

static void write_u64_le(uint8_t *bytes, uint64_t value) {
    uint32_t index = 0;
    while (index < 8) {
        bytes[index] = (uint8_t)(value >> (index * 8));
        index += 1;
    }
}

static void write_f64_le(uint8_t *bytes, double value) {
    uint64_t bits;
    memcpy(&bits, &value, sizeof(bits));
    write_u64_le(bytes, bits);
}

static FfiBuf_u8 encoded_point(double x, double y) {
    uint8_t *bytes = malloc(16);
    write_f64_le(bytes, x);
    write_f64_le(bytes + 8, y);
    return (FfiBuf_u8){bytes, 16, 16, 1};
}

FfiBuf_u8 boltffi_init_record_demo_encoded_point_new(double x, double y) {
    return encoded_point(x, y);
}

double boltffi_method_record_demo_encoded_point_sum(
    const uint8_t *receiver_ptr,
    uintptr_t receiver_len
) {
    if (receiver_len != 16) return -1.0;
    return read_f64_le(receiver_ptr) + read_f64_le(receiver_ptr + 8);
}

FfiStatus boltffi_method_record_demo_encoded_point_scale(
    const uint8_t *receiver_ptr,
    uintptr_t receiver_len,
    FfiBuf_u8 *receiver_out,
    double factor
) {
    if (receiver_len != 16 || receiver_out == NULL) return FFI_STATUS_INVALID_ARG;
    *receiver_out = encoded_point(
        read_f64_le(receiver_ptr) * factor,
        read_f64_le(receiver_ptr + 8) * factor
    );
    return FFI_STATUS_OK;
}

FfiBuf_u8 boltffi_function_demo_round_trip_encoded_point(
    const uint8_t *value_ptr,
    uintptr_t value_len
) {
    return boltffi_buf_from_bytes(value_ptr, value_len);
}

bool boltffi_function_demo_round_trip_bool(bool value) {
    return value;
}

int8_t boltffi_function_demo_round_trip_i8(int8_t value) {
    return value;
}

uint8_t boltffi_function_demo_round_trip_u8(uint8_t value) {
    return value;
}

int16_t boltffi_function_demo_round_trip_i16(int16_t value) {
    return value;
}

uint16_t boltffi_function_demo_round_trip_u16(uint16_t value) {
    return value;
}

#ifndef BOLTFFI_RUNTIME_I32
#define BOLTFFI_RUNTIME_I32(value) (value)
#endif

int32_t boltffi_function_demo_round_trip_i32(int32_t value) {
    return BOLTFFI_RUNTIME_I32(value);
}

uint32_t boltffi_function_demo_round_trip_u32(uint32_t value) {
    return value;
}

int64_t boltffi_function_demo_round_trip_i64(int64_t value) {
    return value;
}

uint64_t boltffi_function_demo_round_trip_u64(uint64_t value) {
    return value;
}

intptr_t boltffi_function_demo_round_trip_isize(intptr_t value) {
    return value;
}

uintptr_t boltffi_function_demo_round_trip_usize(uintptr_t value) {
    return value;
}

float boltffi_function_demo_round_trip_f32(float value) {
    return value;
}

double boltffi_function_demo_round_trip_f64(double value) {
    return value;
}

static uint32_t observed_void_calls = 0;

void boltffi_function_demo_observe_void(void) {
    observed_void_calls += 1;
}

uint32_t boltffi_function_demo_observed_void_calls(void) {
    return observed_void_calls;
}
"#;
const MIXED_C_RUNTIME: &str = r#"
#include <stdint.h>

int32_t boltffi_java_runtime_dependency(void);

#define BOLTFFI_RUNTIME_I32(value) \
    ((value) + boltffi_java_runtime_dependency() - 7)

#include "runtime.c"
"#;
const FALLBACK_C_RUNTIME: &str = r#"
#include <stdint.h>

int32_t boltffi_java_runtime_dependency(void) {
    return 7;
}
"#;
const JAVA_SMOKE_SOURCE: &str = r#"
package __BOLTFFI_PACKAGE__;

public final class RuntimeSmoke {
    private RuntimeSmoke() {}

    public static void main(String[] arguments) {
        if (arguments.length != 0) {
            throw new AssertionError("unexpected arguments");
        }
        Point point = Demo.roundTripPoint(new Point(3.25, -7.5));
        verify(point.x() == 3.25 && point.y() == -7.5, "direct record");

        EncodedPoint encoded = EncodedPoint._new(3.0, 4.0);
        verify(encoded.x() == 3.0 && encoded.y() == 4.0, "encoded initializer");
        verify(encoded.sum() == 7.0, "encoded receiver");
        EncodedPoint scaled = encoded.scale(2.0);
        verify(scaled.x() == 6.0 && scaled.y() == 8.0, "encoded mutation");
        EncodedPoint echoed = Demo.roundTripEncodedPoint(scaled);
        verify(echoed.equals(scaled), "encoded round trip");

        NestedIdentity firstIdentity = new NestedIdentity(
            java.util.Optional.of(new int[] {1, 2, 3}),
            java.util.Collections.singletonList(new byte[] {4, 5}),
            java.util.Arrays.asList(
                java.util.Optional.of(new short[] {6, 7}),
                java.util.Optional.empty()
            )
        );
        NestedIdentity equalIdentity = new NestedIdentity(
            java.util.Optional.of(new int[] {1, 2, 3}),
            java.util.Collections.singletonList(new byte[] {4, 5}),
            java.util.Arrays.asList(
                java.util.Optional.of(new short[] {6, 7}),
                java.util.Optional.empty()
            )
        );
        NestedIdentity differentIdentity = new NestedIdentity(
            java.util.Optional.of(new int[] {1, 2, 4}),
            java.util.Collections.singletonList(new byte[] {4, 5}),
            java.util.Arrays.asList(
                java.util.Optional.of(new short[] {6, 7}),
                java.util.Optional.empty()
            )
        );
        verify(firstIdentity.equals(equalIdentity), "nested identity equality");
        verify(firstIdentity.hashCode() == equalIdentity.hashCode(), "nested identity hash");
        verify(!firstIdentity.equals(differentIdentity), "nested identity difference");

        BoltFFIResult<int[], java.util.Optional<short[]>> firstResult =
            BoltFFIResult.<int[], java.util.Optional<short[]>>ok(new int[] {8, 9});
        BoltFFIResult<int[], java.util.Optional<short[]>> equalResult =
            BoltFFIResult.<int[], java.util.Optional<short[]>>ok(new int[] {8, 9});
        BoltFFIResult<int[], java.util.Optional<short[]>> errorResult =
            BoltFFIResult.<int[], java.util.Optional<short[]>>err(
                java.util.Optional.of(new short[] {10, 11})
            );
        verify(firstResult.isOk(), "result success state");
        verify(firstResult.errValue() == null, "result absent error");
        verify(errorResult.okValue() == null, "result absent value");
        verify(firstResult.equals(equalResult), "result array equality");
        verify(firstResult.hashCode() == equalResult.hashCode(), "result array hash");
        verify(errorResult.toString().startsWith("Err("), "result string");
        verify(
            new ResultHolder(firstResult).equals(new ResultHolder(equalResult)),
            "result record equality"
        );

        verify(Demo.roundTripBool(true), "bool true");
        verify(!Demo.roundTripBool(false), "bool false");

        byte signedByte = (byte) -101;
        verify(Demo.roundTripI8(signedByte) == signedByte, "i8");

        byte unsignedByte = (byte) 0xe1;
        verify(
            Byte.toUnsignedInt(Demo.roundTripU8(unsignedByte)) == 225,
            "u8"
        );

        short signedShort = (short) -23456;
        verify(Demo.roundTripI16(signedShort) == signedShort, "i16");

        short unsignedShort = (short) 0xd234;
        verify(
            Short.toUnsignedInt(Demo.roundTripU16(unsignedShort)) == 53812,
            "u16"
        );

        int signedWord = -1234567890;
        verify(Demo.roundTripI32(signedWord) == signedWord, "i32");

        int unsignedWord = (int) 0xf1234567L;
        verify(
            Integer.toUnsignedLong(Demo.roundTripU32(unsignedWord)) == 4045620583L,
            "u32"
        );

        long signedWide = -8765432109876543210L;
        verify(Demo.roundTripI64(signedWide) == signedWide, "i64");

        long unsignedWide = 0xfedcba9876543210L;
        long returnedUnsignedWide = Demo.roundTripU64(unsignedWide);
        verify(returnedUnsignedWide == unsignedWide, "u64 bits");
        verify(
            Long.toUnsignedString(returnedUnsignedWide).equals("18364758544493064720"),
            "u64 value"
        );

        long signedSize = -1234567L;
        verify(Demo.roundTripIsize(signedSize) == signedSize, "isize");

        long unsignedSize = 2345678901L;
        verify(Demo.roundTripUsize(unsignedSize) == unsignedSize, "usize");

        int singleBits = 0xc2f6e979;
        float single = Float.intBitsToFloat(singleBits);
        verify(
            Float.floatToRawIntBits(Demo.roundTripF32(single)) == singleBits,
            "f32"
        );

        long doubleBits = 0xc05edd2f1a9fbe77L;
        double doubleValue = Double.longBitsToDouble(doubleBits);
        verify(
            Double.doubleToRawLongBits(Demo.roundTripF64(doubleValue)) == doubleBits,
            "f64"
        );

        verify(Integer.toUnsignedLong(Demo.observedVoidCalls()) == 0L, "void initial");
        Demo.observeVoid();
        verify(Integer.toUnsignedLong(Demo.observedVoidCalls()) == 1L, "void first");
        Demo.observeVoid();
        verify(Integer.toUnsignedLong(Demo.observedVoidCalls()) == 2L, "void second");

        verifyWireArrays();
        verifyWireMaps();
        System.out.print(42);
    }

    private static void verifyWireArrays() {
        short[] shorts = {(short) 0x1234};
        int[] integers = {0x12345678};
        long[] longs = {0x0123456789abcdefL};
        float[] floats = {Float.intBitsToFloat(0x41200000)};
        double[] doubles = {Double.longBitsToDouble(0x3ff0000000000000L)};
        WireLease lease = WireWriterPool.acquire(46);
        byte[] bytes;
        try {
            WireWriter writer = lease.writer();
            writer.writeShortArray(shorts);
            writer.writeIntArray(integers);
            writer.writeLongArray(longs);
            writer.writeFloatArray(floats);
            writer.writeDoubleArray(doubles);
            verify(lease.size() == 46, "wire array size");
            bytes = lease.bytes();
        } finally {
            lease.close();
        }
        verify(bytes[0] == 1 && bytes[4] == 0x34 && bytes[5] == 0x12, "short endian");
        verify(bytes[10] == 0x78 && bytes[13] == 0x12, "int endian");
        verify(bytes[18] == (byte) 0xef && bytes[25] == 0x01, "long endian");
        verify(bytes[30] == 0x00 && bytes[33] == 0x41, "float endian");
        verify(bytes[38] == 0x00 && bytes[45] == 0x3f, "double endian");
        WireReader reader = new WireReader(bytes);
        verify(java.util.Arrays.equals(reader.readShortArray(), shorts), "short array");
        verify(java.util.Arrays.equals(reader.readIntArray(), integers), "int array");
        verify(java.util.Arrays.equals(reader.readLongArray(), longs), "long array");
        verify(java.util.Arrays.equals(reader.readFloatArray(), floats), "float array");
        verify(java.util.Arrays.equals(reader.readDoubleArray(), doubles), "double array");
    }

    private static void verifyWireMaps() {
        java.util.LinkedHashMap<Integer, Long> values = new java.util.LinkedHashMap<>();
        values.put(3, 5L);
        values.put(8, 13L);
        int size = WireSizes.map(values, value -> 4, value -> 8);
        verify(size == 28, "wire map size");
        WireLease lease = WireWriterPool.acquire(size);
        byte[] bytes;
        try {
            WireWriter writer = lease.writer();
            writer.writeMap(values, writer::writeInt, writer::writeLong);
            verify(lease.size() == size, "wire map write size");
            bytes = lease.bytes();
        } finally {
            lease.close();
        }
        WireReader reader = new WireReader(bytes);
        java.util.Map<Integer, Long> decoded = reader.readMap(reader::readInt, reader::readLong);
        verify(decoded.equals(values), "wire map round trip");
    }

    private static void verify(boolean condition, String carrier) {
        if (!condition) {
            throw new AssertionError("primitive carrier failed: " + carrier);
        }
    }
}
"#;
const JAVA_CLASS_LOADER_SOURCE: &str = r#"
package __BOLTFFI_PACKAGE__;

public final class ClassLoaderSmoke {
    private ClassLoaderSmoke() {}

    public static void main(String[] arguments) throws Exception {
        if (arguments.length != 1) {
            throw new AssertionError("expected binding directory");
        }
        java.net.URL[] bindings = {
            new java.io.File(arguments[0]).toURI().toURL()
        };
        try (
            java.net.URLClassLoader firstLoader = new java.net.URLClassLoader(bindings, null);
            java.net.URLClassLoader secondLoader = new java.net.URLClassLoader(bindings, null)
        ) {
            int first = roundTripI32(firstLoader);
            int second = roundTripI32(secondLoader);
            System.out.print(first + ":" + second);
        }
    }

    private static int roundTripI32(ClassLoader loader) throws Exception {
        Class<?> demo = Class.forName(
            "__BOLTFFI_PACKAGE__.Demo",
            true,
            loader
        );
        Object result = demo
            .getMethod("roundTripI32", int.class)
            .invoke(null, 42);
        return ((Integer) result).intValue();
    }
}
"#;
const JAVA_LAZY_FALLBACK_SOURCE: &str = r#"
package __BOLTFFI_PACKAGE__;

public final class LazyFallbackSmoke {
    private LazyFallbackSmoke() {}

    public static void main(String[] arguments) throws Exception {
        if (arguments.length != 2) {
            throw new AssertionError("expected binding directory and fallback library");
        }
        java.net.URL[] bindings = {
            new java.io.File(arguments[0]).toURI().toURL()
        };
        try (
            RejectingFallbackLoader loader = new RejectingFallbackLoader(
                bindings,
                System.mapLibraryName(arguments[1])
            )
        ) {
            Class<?> demo = Class.forName(
                "__BOLTFFI_PACKAGE__.Demo",
                true,
                loader
            );
            Object result = demo
                .getMethod("roundTripI32", int.class)
                .invoke(null, 42);
            System.out.print(((Integer) result).intValue());
        }
    }

    private static final class RejectingFallbackLoader
        extends java.net.URLClassLoader {
        private final String fallbackResource;

        private RejectingFallbackLoader(
            java.net.URL[] bindings,
            String fallbackResource
        ) {
            super(bindings, null);
            this.fallbackResource = fallbackResource;
        }

        @Override
        public java.io.InputStream getResourceAsStream(String resourceName) {
            if (resourceName.endsWith(fallbackResource)) {
                throw new AssertionError("fallback resource was accessed");
            }
            return super.getResourceAsStream(resourceName);
        }
    }
}
"#;
const JAVA_ANDROID_SOURCE: &str = r#"
package __BOLTFFI_PACKAGE__;

public final class AndroidSmoke {
    private AndroidSmoke() {}

    public static void main(String[] arguments) {
        if (arguments.length != 0) {
            throw new AssertionError("unexpected arguments");
        }
        System.setProperty("java.vm.name", "Dalvik");
        System.out.print(Demo.roundTripI32(42));
    }
}
"#;
const JAVA_UNSAFE_CACHE_SOURCE: &str = r#"
package __BOLTFFI_PACKAGE__;

public final class UnsafeCacheSmoke {
    private static final String VERSION = "v1";

    private UnsafeCacheSmoke() {}

    public static void main(String[] arguments) throws Exception {
        if (arguments.length != 1) {
            throw new AssertionError("expected cache attack mode");
        }
        java.nio.file.Path temporaryDirectory = java.nio.file.Paths
            .get(System.getProperty("java.io.tmpdir"))
            .toRealPath();
        java.nio.file.Path deterministicCache = temporaryDirectory.resolve(
            "boltffi-native-" + VERSION + "-" + ownerIdentity(temporaryDirectory)
        );
        if (arguments[0].equals("permissions")) {
            java.nio.file.Files.createDirectory(
                deterministicCache,
                java.nio.file.attribute.PosixFilePermissions.asFileAttribute(
                    java.nio.file.attribute.PosixFilePermissions.fromString("rwxrwxrwx")
                )
            );
            java.nio.file.Files.write(
                deterministicCache.resolve("occupied"),
                "unsafe deterministic cache".getBytes(
                    java.nio.charset.StandardCharsets.UTF_8
                ),
                java.nio.file.StandardOpenOption.CREATE_NEW,
                java.nio.file.StandardOpenOption.WRITE
            );
        } else if (arguments[0].equals("symlink")) {
            java.nio.file.Path target = temporaryDirectory.resolve("attacker-cache");
            java.nio.file.Files.createDirectory(target);
            java.nio.file.Files.createSymbolicLink(deterministicCache, target);
        } else {
            throw new AssertionError("unknown cache attack mode");
        }
        System.out.print(Demo.roundTripI32(42));
    }

    private static String ownerIdentity(
        java.nio.file.Path temporaryDirectory
    ) throws Exception {
        java.nio.file.Path probe = java.nio.file.Files.createTempFile(
            temporaryDirectory,
            "boltffi-owner-test-",
            ".probe"
        );
        try {
            java.nio.file.attribute.UserPrincipal owner = java.nio.file.Files.getOwner(
                probe,
                java.nio.file.LinkOption.NOFOLLOW_LINKS
            );
            java.security.MessageDigest digest = java.security.MessageDigest.getInstance(
                "SHA-256"
            );
            updateIdentity(digest, owner.getName());
            return hex(digest.digest());
        } finally {
            java.nio.file.Files.deleteIfExists(probe);
        }
    }

    private static void updateIdentity(
        java.security.MessageDigest digest,
        String value
    ) {
        byte[] encoded = value.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        digest.update((byte) (encoded.length >>> 24));
        digest.update((byte) (encoded.length >>> 16));
        digest.update((byte) (encoded.length >>> 8));
        digest.update((byte) encoded.length);
        digest.update(encoded);
    }

    private static String hex(byte[] bytes) {
        char[] digits = "0123456789abcdef".toCharArray();
        char[] encoded = new char[bytes.length * 2];
        int index = 0;
        while (index < bytes.length) {
            int value = bytes[index] & 0xff;
            encoded[index * 2] = digits[value >>> 4];
            encoded[index * 2 + 1] = digits[value & 0x0f];
            index += 1;
        }
        return new String(encoded);
    }
}
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativePlatform {
    Linux,
    MacOs,
}

impl NativePlatform {
    fn current() -> Option<Self> {
        if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "macos") {
            Some(Self::MacOs)
        } else {
            None
        }
    }

    fn jni_include_directory(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::MacOs => "darwin",
        }
    }

    fn library_filename(self, library_name: &str) -> String {
        match self {
            Self::Linux => format!("lib{library_name}.so"),
            Self::MacOs => format!("lib{library_name}.dylib"),
        }
    }

    fn configure_compiler(self, compiler: &mut Command) {
        match self {
            Self::Linux => {
                compiler.args(["-shared", "-fPIC"]);
            }
            Self::MacOs => {
                compiler.arg("-dynamiclib");
            }
        }
    }

    fn configure_library_identity(self, compiler: &mut Command, library_name: &str) {
        let filename = self.library_filename(library_name);
        match self {
            Self::Linux => {
                compiler.arg(format!("-Wl,-soname,{filename}"));
            }
            Self::MacOs => {
                compiler.arg(format!("-Wl,-install_name,{filename}"));
            }
        }
    }

    fn compiler_available() -> bool {
        Command::new("cc")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JavaToolchain {
    compiler: JavaCompiler,
    home: PathBuf,
}

impl JavaToolchain {
    fn discover() -> Option<Self> {
        let compiler = JavaCompiler::discover()?;
        let settings = Command::new("javac")
            .args(["-J-XshowSettings:properties", "-version"])
            .output()
            .ok()?;
        if !settings.status.success() {
            return None;
        }
        let home = [&settings.stderr, &settings.stdout]
            .into_iter()
            .find_map(|bytes| {
                String::from_utf8_lossy(bytes)
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("java.home = ").map(PathBuf::from))
            })?;

        Some(Self { compiler, home })
    }

    fn compiler(&self) -> &JavaCompiler {
        &self.compiler
    }

    fn runtime_available() -> bool {
        Command::new("java")
            .arg("-version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn jni_includes(&self, platform_directory: &str) -> Option<JniIncludes> {
        JniIncludes::discover(&self.home, platform_directory)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JniIncludes {
    common: PathBuf,
    platform: PathBuf,
}

impl JniIncludes {
    fn discover(java_home: &Path, platform_directory: &str) -> Option<Self> {
        [Some(java_home), java_home.parent()]
            .into_iter()
            .flatten()
            .find_map(|home| Self::from_home(home, platform_directory))
    }

    fn common(&self) -> &Path {
        &self.common
    }

    fn platform(&self) -> &Path {
        &self.platform
    }

    fn from_home(home: &Path, platform_directory: &str) -> Option<Self> {
        let common = home.join("include");
        let platform = common.join(platform_directory);
        (common.join("jni.h").is_file() && platform.join("jni_md.h").is_file())
            .then_some(Self { common, platform })
    }
}

#[derive(Debug)]
struct RuntimeFixture {
    root: PathBuf,
    classes: PathBuf,
    bundled_libraries: PathBuf,
    native_libraries: PathBuf,
    temporary_files: PathBuf,
    platform: NativePlatform,
    toolchain: JavaToolchain,
    jni_includes: JniIncludes,
}

impl RuntimeFixture {
    fn discover() -> Option<Self> {
        let platform = NativePlatform::current()?;
        let toolchain = JavaToolchain::discover()?;
        if !JavaToolchain::runtime_available() || !NativePlatform::compiler_available() {
            return None;
        }
        let jni_includes = toolchain.jni_includes(platform.jni_include_directory())?;

        Some(Self::new(platform, toolchain, jni_includes))
    }

    fn new(platform: NativePlatform, toolchain: JavaToolchain, jni_includes: JniIncludes) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let sequence = NEXT_RUNTIME_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "boltffi-java-runtime-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        let fixture = Self {
            classes: root.join("classes"),
            bundled_libraries: root.join("bundled-libraries"),
            native_libraries: root.join("native-libraries"),
            temporary_files: root.join("temporary"),
            root,
            platform,
            toolchain,
            jni_includes,
        };
        [
            &fixture.root,
            &fixture.classes,
            &fixture.bundled_libraries,
            &fixture.native_libraries,
            &fixture.temporary_files,
        ]
        .into_iter()
        .try_for_each(fs::create_dir)
        .expect("create Java runtime fixture directories");
        fixture
    }

    fn prepare(&self) {
        self.generate_bindings(LIBRARY_NAME, LIBRARY_NAME);
        self.compile_native_library();
        self.compile_java();
    }

    fn prepare_mixed_libraries(&self) {
        self.generate_bindings(PREFERRED_LIBRARY_NAME, FALLBACK_LIBRARY_NAME);
        self.compile_fallback_library();
        self.compile_preferred_library();
        self.copy_preferred_bundled_library();
        self.compile_java();
    }

    fn prepare_bundled_pair(&self) {
        self.prepare_mixed_libraries();
        fs::copy(
            self.system_library(FALLBACK_LIBRARY_NAME),
            self.classes
                .join(self.platform.library_filename(FALLBACK_LIBRARY_NAME)),
        )
        .expect("copy bundled fallback native library");
    }

    fn prepare_lazy_fallback(&self) {
        self.generate_bindings(PREFERRED_LIBRARY_NAME, FALLBACK_LIBRARY_NAME);
        self.compile_native_library();
        fs::copy(
            self.native_library(),
            self.bundled_library(PREFERRED_LIBRARY_NAME),
        )
        .expect("copy standalone preferred native library");
        self.copy_preferred_bundled_library();
        self.compile_java();
    }

    fn prepare_android_without_desktop_runtime(&self) {
        self.prepare();
        fs::remove_file(
            self.classes
                .join(JAVA_PACKAGE.replace('.', "/"))
                .join("BoltFFINativeRuntime.class"),
        )
        .expect("remove desktop runtime class");
    }

    fn generate_bindings(&self, preferred_library: &str, fallback_library: &str) {
        let file = syn::parse_str(RUST_SOURCE).expect("valid Java runtime source fixture");
        let scanned = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
            .expect("Java runtime fixture should scan");
        let bindings = lower::<Native>(&scanned).expect("Java runtime fixture should lower");
        let host = JavaHost::new(JAVA_PACKAGE, "Demo")
            .expect("Java runtime host")
            .c_header("native/boltffi.h")
            .jni_source("native/jni_glue.c")
            .android_library(fallback_library)
            .expect("Android library name")
            .desktop_jni_library(preferred_library)
            .expect("desktop JNI library name")
            .desktop_fallback_library(fallback_library)
            .expect("desktop fallback library name");
        let output = host
            .render_with_coverage(&bindings, CoverageMode::Complete)
            .expect("Java runtime target should render");

        output
            .files()
            .iter()
            .try_for_each(|generated| {
                let destination = self.root.join(generated.path().as_path());
                fs::create_dir_all(destination.parent().expect("generated file parent"))?;
                fs::write(destination, generated.contents())
            })
            .expect("write generated Java runtime files");
        fs::write(self.root.join("runtime.c"), C_RUNTIME).expect("write C runtime fixture");
        fs::write(self.root.join("mixed_runtime.c"), MIXED_C_RUNTIME)
            .expect("write mixed C runtime fixture");
        fs::write(self.root.join("fallback_runtime.c"), FALLBACK_C_RUNTIME)
            .expect("write fallback C runtime fixture");
        let package_directory = self.package_directory();
        fs::write(
            package_directory.join("RuntimeSmoke.java"),
            Self::java_source(JAVA_SMOKE_SOURCE),
        )
        .expect("write Java smoke source");
        fs::write(
            package_directory.join("ClassLoaderSmoke.java"),
            Self::java_source(JAVA_CLASS_LOADER_SOURCE),
        )
        .expect("write Java classloader smoke source");
        fs::write(
            package_directory.join("UnsafeCacheSmoke.java"),
            Self::java_source(JAVA_UNSAFE_CACHE_SOURCE),
        )
        .expect("write Java unsafe cache smoke source");
        fs::write(
            package_directory.join("LazyFallbackSmoke.java"),
            Self::java_source(JAVA_LAZY_FALLBACK_SOURCE),
        )
        .expect("write Java lazy fallback smoke source");
        fs::write(
            package_directory.join("AndroidSmoke.java"),
            Self::java_source(JAVA_ANDROID_SOURCE),
        )
        .expect("write Java Android smoke source");
    }

    fn compile_native_library(&self) {
        let mut compiler = Command::new("cc");
        compiler.args(["-std=c11", "-O2"]);
        self.platform.configure_compiler(&mut compiler);
        compiler
            .arg("-I")
            .arg(self.jni_includes.common())
            .arg("-I")
            .arg(self.jni_includes.platform())
            .arg(self.root.join("native/jni_glue.c"))
            .arg(self.root.join("runtime.c"))
            .arg("-o")
            .arg(self.native_library());
        self.execute(&mut compiler, "native library compilation");
    }

    fn compile_fallback_library(&self) {
        let mut compiler = Command::new("cc");
        compiler.args(["-std=c11", "-O2"]);
        self.platform.configure_compiler(&mut compiler);
        self.platform
            .configure_library_identity(&mut compiler, FALLBACK_LIBRARY_NAME);
        compiler
            .arg(self.root.join("fallback_runtime.c"))
            .arg("-o")
            .arg(self.system_library(FALLBACK_LIBRARY_NAME));
        self.execute(&mut compiler, "fallback native library compilation");
    }

    fn compile_preferred_library(&self) {
        let mut compiler = Command::new("cc");
        compiler.args(["-std=c11", "-O2"]);
        self.platform.configure_compiler(&mut compiler);
        self.platform
            .configure_library_identity(&mut compiler, PREFERRED_LIBRARY_NAME);
        compiler
            .arg("-I")
            .arg(self.jni_includes.common())
            .arg("-I")
            .arg(self.jni_includes.platform())
            .arg(self.root.join("native/jni_glue.c"))
            .arg(self.root.join("mixed_runtime.c"))
            .arg("-L")
            .arg(&self.native_libraries)
            .arg(format!("-l{FALLBACK_LIBRARY_NAME}"))
            .arg("-o")
            .arg(self.bundled_library(PREFERRED_LIBRARY_NAME));
        self.execute(&mut compiler, "preferred native library compilation");
    }

    fn compile_java(&self) {
        let mut sources = fs::read_dir(self.package_directory())
            .expect("read generated Java package")
            .map(|entry| entry.expect("generated Java package entry").path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "java")
            })
            .collect::<Vec<_>>();
        sources.sort();
        let mut compiler = Command::new("javac");
        compiler.args(["-encoding", "UTF-8"]);
        self.toolchain
            .compiler()
            .configure_java_eight(&mut compiler);
        compiler.arg("-d").arg(&self.classes).args(sources);
        self.execute(&mut compiler, "Java 8 source compilation");
    }

    fn write_corrupt_bundled_library(&self) {
        fs::write(
            self.classes
                .join(self.platform.library_filename(LIBRARY_NAME)),
            CORRUPT_LIBRARY,
        )
        .expect("write corrupt bundled native library");
    }

    fn write_valid_bundled_library(&self) {
        fs::copy(
            self.native_library(),
            self.classes
                .join(self.platform.library_filename(LIBRARY_NAME)),
        )
        .expect("copy bundled native library");
    }

    fn copy_preferred_bundled_library(&self) {
        fs::copy(
            self.bundled_library(PREFERRED_LIBRARY_NAME),
            self.classes
                .join(self.platform.library_filename(PREFERRED_LIBRARY_NAME)),
        )
        .expect("copy preferred bundled native library");
    }

    fn run_system_fallback(&self) {
        let mut java = Command::new("java");
        java.arg(Self::property("java.io.tmpdir", &self.temporary_files))
            .arg(Self::property("java.library.path", &self.native_libraries))
            .arg("-cp")
            .arg(&self.classes)
            .arg(Self::qualified_class("RuntimeSmoke"));
        let output = self.execute(&mut java, "generated Java runtime execution");

        assert_eq!(output.stdout, b"42");
    }

    fn run_mixed_library_sources(&self) {
        let mut java = Command::new("java");
        java.arg(Self::property("java.io.tmpdir", &self.temporary_files))
            .arg(Self::property("java.library.path", &self.native_libraries))
            .arg("-cp")
            .arg(&self.classes)
            .arg(Self::qualified_class("RuntimeSmoke"));
        let output = self.execute(&mut java, "mixed-source Java runtime execution");

        assert_eq!(output.stdout, b"42");
    }

    fn run_bundled_pair(&self) {
        let mut java = Command::new("java");
        java.arg(Self::property("java.io.tmpdir", &self.temporary_files))
            .arg(Self::property("java.library.path", &self.temporary_files))
            .arg("-cp")
            .arg(&self.classes)
            .arg(Self::qualified_class("RuntimeSmoke"));
        let output = self.execute(&mut java, "bundled-pair Java runtime execution");

        assert_eq!(output.stdout, b"42");
    }

    fn run_lazy_fallback(&self) {
        let mut java = Command::new("java");
        java.arg(Self::property("java.io.tmpdir", &self.temporary_files))
            .arg(Self::property("java.library.path", &self.temporary_files))
            .arg("-cp")
            .arg(&self.classes)
            .arg(Self::qualified_class("LazyFallbackSmoke"))
            .arg(&self.classes)
            .arg(FALLBACK_LIBRARY_NAME);
        let output = self.execute(&mut java, "lazy fallback Java runtime execution");

        assert_eq!(output.stdout, b"42");
    }

    fn run_android_without_desktop_runtime(&self) {
        let mut java = Command::new("java");
        java.arg(Self::property("java.library.path", &self.native_libraries))
            .arg("-cp")
            .arg(&self.classes)
            .arg(Self::qualified_class("AndroidSmoke"));
        let output = self.execute(&mut java, "Android owner-path Java runtime execution");

        assert_eq!(output.stdout, b"42");
    }

    fn assert_mixed_library_sources(&self) {
        assert!(self.bundled_library(PREFERRED_LIBRARY_NAME).is_file());
        assert!(
            self.classes
                .join(self.platform.library_filename(PREFERRED_LIBRARY_NAME))
                .is_file()
        );
        assert!(!self.system_library(PREFERRED_LIBRARY_NAME).exists());
        assert!(self.system_library(FALLBACK_LIBRARY_NAME).is_file());
        assert!(
            !self
                .classes
                .join(self.platform.library_filename(FALLBACK_LIBRARY_NAME))
                .exists()
        );
    }

    fn run_isolated_class_loaders(&self) {
        let mut java = Command::new("java");
        java.arg(Self::property("java.io.tmpdir", &self.temporary_files))
            .arg("-cp")
            .arg(&self.classes)
            .arg(Self::qualified_class("ClassLoaderSmoke"))
            .arg(&self.classes);
        let output = self.execute(&mut java, "isolated Java classloader execution");

        assert_eq!(output.stdout, b"42:42");
    }

    fn run_with_unsafe_deterministic_path(&self) {
        let mut java = Command::new("java");
        java.arg(Self::property("java.io.tmpdir", &self.temporary_files))
            .arg(Self::property("java.library.path", &self.temporary_files))
            .arg(Self::property("user.name", "spoofed-cache-owner"))
            .arg(Self::property(
                "user.home",
                self.temporary_files.join("spoofed-home"),
            ))
            .arg("-cp")
            .arg(&self.classes)
            .arg(Self::qualified_class("UnsafeCacheSmoke"))
            .arg("permissions");
        let output = self.execute(&mut java, "unsafe deterministic path execution");

        assert_eq!(output.stdout, b"42");
    }

    fn run_with_deterministic_symlink(&self) {
        let mut java = Command::new("java");
        java.arg(Self::property("java.io.tmpdir", &self.temporary_files))
            .arg(Self::property("java.library.path", &self.temporary_files))
            .arg(Self::property("user.name", "spoofed-cache-owner"))
            .arg(Self::property(
                "user.home",
                self.temporary_files.join("spoofed-home"),
            ))
            .arg("-cp")
            .arg(&self.classes)
            .arg(Self::qualified_class("UnsafeCacheSmoke"))
            .arg("symlink");
        let output = self.execute(&mut java, "deterministic symlink execution");

        assert_eq!(output.stdout, b"42");
    }

    fn assert_unsafe_deterministic_path_was_ignored(&self) {
        let entries = fs::read_dir(&self.temporary_files)
            .expect("native extraction parent should exist")
            .map(|entry| entry.expect("native extraction root entry").path())
            .collect::<Vec<_>>();
        let deterministic = entries
            .iter()
            .find(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(Self::is_deterministic_cache_root_name)
            })
            .expect("unsafe deterministic cache path should remain occupied");
        assert_eq!(
            fs::read(deterministic.join(UNSAFE_CACHE_ENTRY))
                .expect("read unsafe deterministic cache entry"),
            UNSAFE_CACHE
        );
        #[cfg(unix)]
        assert_ne!(
            fs::metadata(deterministic)
                .expect("unsafe deterministic cache metadata")
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }

    fn assert_deterministic_symlink_was_ignored(&self) {
        let entries = fs::read_dir(&self.temporary_files)
            .expect("native extraction parent should exist")
            .map(|entry| entry.expect("native extraction root entry").path())
            .collect::<Vec<_>>();
        let deterministic = entries
            .iter()
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(Self::is_deterministic_cache_root_name)
            })
            .expect("deterministic cache symlink should remain occupied");
        assert!(
            fs::symlink_metadata(deterministic)
                .expect("deterministic cache symlink metadata")
                .file_type()
                .is_symlink()
        );
        let attacker = self.temporary_files.join(SYMLINK_CACHE_TARGET);
        assert!(attacker.is_dir());
        assert!(
            Self::extracted_libraries_in(&attacker, &self.platform.library_filename(LIBRARY_NAME),)
                .is_empty(),
            "attacker-controlled symlink target must not receive a native library"
        );
    }

    #[cfg(unix)]
    fn assert_extraction_cleanup(&self) {
        assert!(
            fs::read_dir(&self.temporary_files)
                .expect("native extraction parent should exist")
                .map(|entry| entry.expect("native extraction root entry").path())
                .filter_map(|path| path.file_name().and_then(OsStr::to_str).map(str::to_owned))
                .all(|name| !Self::is_extraction_root_name(&name)),
            "native extraction directories should be removed when the Java process exits"
        );
    }

    fn extracted_libraries_in(directory: &Path, mapped_name: &str) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .unwrap_or_else(|failure| {
                panic!(
                    "could not read native extraction directory '{}': {failure}",
                    directory.display()
                )
            })
            .map(|entry| entry.expect("native extraction entry").path())
            .flat_map(|path| {
                if path.is_dir() {
                    Self::extracted_libraries_in(&path, mapped_name)
                } else if path.file_name().is_some_and(|name| name == mapped_name) {
                    vec![path]
                } else {
                    Vec::new()
                }
            })
            .collect()
    }

    fn is_deterministic_cache_root_name(name: &str) -> bool {
        let Some(suffix) = name.strip_prefix("boltffi-native-v1-") else {
            return false;
        };
        suffix.len() == 64
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    fn is_extraction_root_name(name: &str) -> bool {
        name.strip_prefix("boltffi-native-")
            .is_some_and(|suffix| !suffix.is_empty())
            && !Self::is_deterministic_cache_root_name(name)
    }

    fn property(name: &str, value: impl AsRef<OsStr>) -> OsString {
        let mut property = OsString::from("-D");
        property.push(name);
        property.push("=");
        property.push(value.as_ref());
        property
    }

    fn package_directory(&self) -> PathBuf {
        self.root.join(JAVA_PACKAGE.replace('.', "/"))
    }

    fn qualified_class(simple_name: &str) -> String {
        format!("{JAVA_PACKAGE}.{simple_name}")
    }

    fn java_source(template: &str) -> String {
        template.replace("__BOLTFFI_PACKAGE__", JAVA_PACKAGE)
    }

    fn native_library(&self) -> PathBuf {
        self.system_library(LIBRARY_NAME)
    }

    fn system_library(&self, library_name: &str) -> PathBuf {
        self.native_libraries
            .join(self.platform.library_filename(library_name))
    }

    fn bundled_library(&self, library_name: &str) -> PathBuf {
        self.bundled_libraries
            .join(self.platform.library_filename(library_name))
    }

    fn execute(&self, command: &mut Command, operation: &str) -> Output {
        let output = command
            .current_dir(&self.root)
            .output()
            .unwrap_or_else(|failure| panic!("{operation} could not start: {failure}"));
        assert!(
            output.status.success(),
            "{operation} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

impl Drop for RuntimeFixture {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.root));
    }
}

#[test]
fn generated_java_runtime_falls_back_from_corrupt_bundled_library() {
    let Some(fixture) = RuntimeFixture::discover() else {
        return;
    };

    fixture.prepare();
    fixture.write_corrupt_bundled_library();
    fixture.run_system_fallback();
    #[cfg(unix)]
    fixture.assert_extraction_cleanup();
}

#[test]
fn generated_java_runtime_preserves_per_library_source_order() {
    let Some(fixture) = RuntimeFixture::discover() else {
        return;
    };

    fixture.prepare_mixed_libraries();
    fixture.assert_mixed_library_sources();
    fixture.run_mixed_library_sources();
    #[cfg(unix)]
    fixture.assert_extraction_cleanup();
}

#[test]
fn generated_java_runtime_co_locates_bundled_fallback_before_preferred_retry() {
    let Some(fixture) = RuntimeFixture::discover() else {
        return;
    };

    fixture.prepare_bundled_pair();
    fixture.run_bundled_pair();
    #[cfg(unix)]
    fixture.assert_extraction_cleanup();
}

#[test]
fn generated_java_runtime_does_not_touch_fallback_after_preferred_success() {
    let Some(fixture) = RuntimeFixture::discover() else {
        return;
    };

    fixture.prepare_lazy_fallback();
    fixture.run_lazy_fallback();
    #[cfg(unix)]
    fixture.assert_extraction_cleanup();
}

#[test]
fn generated_java_android_path_does_not_resolve_the_desktop_runtime() {
    let Some(fixture) = RuntimeFixture::discover() else {
        return;
    };

    fixture.prepare_android_without_desktop_runtime();
    fixture.run_android_without_desktop_runtime();
}

#[test]
fn generated_java_runtime_loads_from_two_isolated_classloaders() {
    let Some(fixture) = RuntimeFixture::discover() else {
        return;
    };

    fixture.prepare();
    fixture.write_valid_bundled_library();
    fixture.run_isolated_class_loaders();
    #[cfg(unix)]
    fixture.assert_extraction_cleanup();
}

#[test]
fn generated_java_runtime_ignores_an_unsafe_deterministic_path() {
    let Some(fixture) = RuntimeFixture::discover() else {
        return;
    };

    fixture.prepare();
    fixture.write_valid_bundled_library();
    fixture.run_with_unsafe_deterministic_path();
    fixture.assert_unsafe_deterministic_path_was_ignored();
    #[cfg(unix)]
    fixture.assert_extraction_cleanup();
}

#[test]
fn generated_java_runtime_ignores_a_deterministic_symlink() {
    let Some(fixture) = RuntimeFixture::discover() else {
        return;
    };

    fixture.prepare();
    fixture.write_valid_bundled_library();
    fixture.run_with_deterministic_symlink();
    fixture.assert_deterministic_symlink_was_ignored();
    #[cfg(unix)]
    fixture.assert_extraction_cleanup();
}

#[test]
fn finds_jni_headers_above_a_java_eight_runtime_home() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "boltffi-java-eight-home-{}-{nonce}",
        std::process::id()
    ));
    let jdk = root.join("jdk");
    let runtime_home = jdk.join("jre");
    let common_include = jdk.join("include");
    let platform_include = common_include.join("platform");
    fs::create_dir_all(&runtime_home).expect("create Java 8 runtime home");
    fs::create_dir_all(&platform_include).expect("create Java 8 JNI includes");
    fs::write(common_include.join("jni.h"), []).expect("write JNI header");
    fs::write(platform_include.join("jni_md.h"), []).expect("write platform JNI header");

    let includes = JniIncludes::discover(&runtime_home, "platform")
        .expect("JNI headers above Java 8 runtime home");

    assert_eq!(includes.common(), common_include);
    assert_eq!(includes.platform(), platform_include);
    fs::remove_dir_all(root).expect("remove Java 8 home fixture");
}
