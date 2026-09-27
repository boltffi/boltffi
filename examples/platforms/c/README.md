# C example

This directory calls the Rust demo library through its generated C API. Start with [example.c](example.c): it passes a string and a record, uses a class, checks a result, and frees the returned owners.

The C target is experimental. It supports synchronous functions and methods, records, enums with payloads, collections, custom types, and constants. See [memory management](https://boltffi.dev/docs/c#memory-management), [callbacks](https://boltffi.dev/docs/callbacks#c-vtables), and the [current limitations](https://boltffi.dev/docs/experimental#c).

## Run

The build needs Rust, a C11 compiler, and CMake 3.20 or later. From the repository root:

```sh
bash examples/platforms/c/test-demo.sh
```

The script builds the Rust library with the `c-demo` feature, generates `generated/include/demo.h`, and packages the native libraries and build metadata under `generated/lib`. It tests repackaging with `--no-build`, moves the package into a directory whose name contains a space, and runs the example and full test suite with both shared and static linking. It restores a copy under `generated/` when it finishes. The feature includes extra records used to exercise mutable C values.

When pkg-config is installed, the CMake build also checks both generated `.pc` files. When Meson is installed on Linux or macOS, the script tests its shared and static consumers too. Use pkgconf 3.0.7 or newer for these checks if your installation has the older whitespace-escaping bug. CI installs these tools for the Unix demo jobs.

The same build runs in CI on Linux, macOS, and Windows. On Windows, use an environment with the MSVC toolchain available. The cross-platform demo runner can also select C:

```sh
bash examples/demo/verify-platform-demos.sh --platform c
```

After the script finishes, run the example again with:

```sh
./examples/platforms/c/build/c_example
```

With a multi-configuration generator such as Visual Studio, the executable is under `build/Debug/c_example.exe`.

## Use the package elsewhere

Include `demo.h` and let the generated package supply the include and link settings:

```cmake
find_package(demo CONFIG REQUIRED)
target_link_libraries(your_app PRIVATE demo::demo)
```

Pass `-DCMAKE_PREFIX_PATH=/path/to/generated` when configuring your project. Use `demo::demo_static` for static linking; it includes the system libraries reported by Rust. The demo's CMake project shows how to copy the DLL beside a Windows executable.

Make and Meson consumers can add `generated/lib/pkgconfig` to `PKG_CONFIG_PATH`. The `demo` module describes normal linking; `demo-static` selects the archive under `lib/static/`. [meson.build](meson.build) shows both dependencies.

The [linking guide](https://boltffi.dev/docs/c#link-a-c-program) covers each build system and runtime library paths. A consuming project needs only the generated package, not this demo's build files.

## Further examples

The [tests](tests) contain executable examples of strings and byte buffers, optional values, direct and owning records, enums, nested collections and maps, custom types, results, constants, class handles, and callback ownership. Each file covers one API category. Async functions, streams, and callbacks needing unsupported conversions are recorded as gaps in the demo coverage audit, not represented as passing tests.
