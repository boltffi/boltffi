# Per-invocation expansion

Every BoltFFI macro invocation describes and expands only its own item. Nothing reads the
crate's source files: the compiler resolves every name, including names that
`macro_rules!` or `include!` produce. Bindgen reads the same descriptions back from the
compiled artifact.

This page follows one small crate through the compiler. The outputs are real expansions,
trimmed: `…` marks cut code, and `::boltffi::__private::` is shortened.

```rust
boltffi::scaffolding!();

pub mod shapes {
    #[data] pub struct Point { pub x: f64, pub y: f64 }
    #[data] pub struct Line { pub a: Point, pub b: Point }
}

use shapes::Line;

#[export]
pub fn length(line: Line) -> f64 { … }
```

The `#[export]` on `length` must write an `extern "C"` wrapper, and its signature depends
on how `Line` crosses. A direct record takes one argument by value, an encoded one takes a
pointer and a length. That in turn depends on `Point`. The macro receives only the tokens
of `length` and sees neither type.

`boltffi::scaffolding!()` goes once at the root of each participating crate. It defines a
crate-local tag so `custom_type!` can register foreign types without violating Rust's
orphan rules.

## Fragments

A fragment (`SourceFragment`) is the description of one annotated item, built by that
item's macro from its tokens alone. For `Line`:

```json
{ "record": {
    "id": "$self::Line",
    "fields": [
      { "name": "a", "type_expr": { "Record": { "id": "$slot:0", "path": "Point" } } },
      { "name": "b", "type_expr": { "Record": { "id": "$slot:0", "path": "Point" } } } ] } }
```

A fragment is partial, with two holes:

- `$self`: the macro does not know the module it runs in.
- `$slot:N`: the macro sees the word `Point`, not which `Point` it is or what kind of item.

Every name a fragment refers to becomes a slot, and slots are filled from outside the
macro. Records fill them for bindgen, and lanes fill them for other invocations.

## Records

Each invocation writes its fragment into the compiled artifact as a `static` in the
`__boltffisrc` link section. Nothing reads it at runtime. Slot values are written as trait
constants, and rustc evaluates them:

```rust
// emitted by #[data] Line
const _: () = {
    const SLOT_0: &[u8] = <Point as TypeDesc<crate::__BoltffiTag>>::DESC.as_bytes();
    #[link_section = "__DATA,__boltffisrc"] #[used]
    static RECORD: Record = Record {
        module: *b"geo::shapes",                                 // from module_path!()
        slot_0: *SLOT_0.first_chunk().unwrap(),                  // rustc: "geo::shapes::Point"
        json: *b"{\"record\":{\"id\":\"$self::Line\",…}}",
        …
    };
};
```

The macro only wrote the path `Point`. The compiler resolved it through imports, renames
and re-exports, and filled in which `Point` that is.

`boltffi generate` runs one ordinary `cargo build` and reads every record from the
artifact (`boltffi_bindgen/src/artifact.rs`). `aggregate_records` then:

- fills `$self` from the record's crate;
- fills each slot from the value rustc wrote, with the module dropped
  (`geo::shapes::Point` becomes `geo::Point`);
- merges the records into a `SourceContract`, which is then lowered.

Most of each record is written as literal bytes, so rustc's const evaluation stays small.
Building a record with `const fn` calls costs about a millisecond per record.

## Lanes

Records reach bindgen only after the build. A macro needs the same facts during the build,
so every type BoltFFI declares also emits a macro under the type's own name:

```rust
// emitted by #[data] Point, beside the struct
#[macro_export]
macro_rules! __boltffi_lane_geo_Point_0 {
    ([$($callback:tt)*] { $($state:tt)* }) => {
        $($callback)*! { $($state)* r#"{"id":"geo::Point","entries":[…]}"# }
    };
}
pub use __boltffi_lane_geo_Point_0 as Point;
```

This is the lane. It takes a macro to call and some tokens, then calls that macro with the
tokens plus a JSON literal appended:

```rust
Point! { [show] { hello } }
// expands to
show! { hello r#"{"id":"geo::Point","entries":[…]}"# }
```

Rust keeps macros and types in separate namespaces, so the `pub use … as Point` puts a
macro named `Point` next to the struct named `Point`. Any path that names the type also
names its lane: imports, renames, globs, nested paths, re-exports, and other crates
through `#[macro_export]`.

The literal carries the type's own fragment only, as a lookup entry whose slots stay
unresolved. `Line`'s lane does not mention which `Point` its fields hold:

```json
{ "id": "geo::Line",
  "entries": [
    { "module": "geo", "slots": [], "lookup": true,
      "json": { "record": { "id": "$self::Line", "fields": [ … "$slot:0" …, … "$slot:0" … ] } } } ] }
```

One level is enough. Lowering an item reads the own fields, repr and variants of the types
it names, and classes and callbacks only by id, never what those types refer to in turn.
Because a lane needs nothing but its own fragment, it is defined as soon as the attribute
runs, before any chain. Types that refer to each other therefore never wait on one
another:

```rust
#[data] pub struct Folder { pub name: String, pub files: Vec<File> }
#[data] pub struct File { pub name: String, pub subfolders: Vec<Folder> }
```

`custom_type!` and `#[custom_ffi]` are the exception. Their lane carries the entries their
representation lowers through, so it is defined when their chain finishes.

## One invocation, step by step

`#[data] Line` needs `Point`, so it runs in three expansions.

**1. The attribute** emits the struct, its record, its own lane, and a call to `Point`'s
lane:

```rust
#[repr(C)] pub struct Line { pub a: Point, pub b: Point }
const _: () = { … static RECORD … };

#[macro_export]
macro_rules! __boltffi_lane_geo_Line_1 { … }
pub use __boltffi_lane_geo_Line_1 as Line;

Point! {
    [::boltffi::__private::lane_resume]
    { data {} { pub struct Line { pub a: Point, pub b: Point } } [] [] }
}
```

The state is `kind { attribute } { item } [resolved lanes] [pending lanes]`.

**2. The lane** is a `macro_rules!`, so rustc substitutes it without running any
proc-macro code:

```rust
::boltffi::__private::lane_resume! {
    data {} { pub struct Line { pub a: Point, pub b: Point } } [] []
    r#"{"id":"geo::Point","entries":[…]}"#
}
```

**3. `lane_resume`** moves the appended literal into the resolved list. If a lane is still
pending, it calls that lane the same way. Here none is, so it finishes:

- It builds the invocation's own entry, with its slots resolved, and adds the lookup
  entries of the lanes it called: `Line`, with `Point` for lookup.
- It runs `aggregate_invocation`, then `lower_invocation` for `Native` and `Wasm32`, then
  the expander. These are bindgen's `aggregate_records` and lowering, restricted to the
  invocation's own item: the lookup entries answer id, kind and shape, and are not
  lowered themselves.
- It renders only its own item. `Line` has a record field, so it crosses encoded:

```rust
const _: () = {
    unsafe impl WirePassable for Line {}
    impl WireEncode for Line { … self.a.encode_to(…); self.b.encode_to(…) … }
    impl WireDecode for Line { … }
};
```

`#[export] length` follows the same path with one hop through `Line!`. It lowers `length`
against `Line`'s own fields, without reaching `Point`, and renders its wrapper:

```rust
pub mod __boltffi_fn_length {
    use super::*;
    #[no_mangle]
    pub unsafe extern "C" fn boltffi_function_geo_length(
        line_ptr: *const u8,
        line_len: usize,
    ) -> f64 {
        let line: Line = match wire::decode::<Line>(slice::from_raw_parts(line_ptr, line_len)) { … };
        length(line)
    }
}
pub use __boltffi_fn_length::*;
```

A `#[cfg(target_arch = "wasm32")]` twin, lowered for `Wasm32`, sits beside it.

When a signature names several types, the first expansion calls the first lane and lists
the rest as pending:

```rust
// fn f(a: Line, c: Circle)
Line! { [lane_resume] { export {} { … } [] [(Circle)] } }
// after Line's hop, lane_resume emits
Circle! { [lane_resume] { export {} { … } [r#"…Line…"#] [] } }
```

That is one hop per distinct type the item names. Types reached only through another
type, like `Point` for `length`, are not visited.

Bindgen, reading the three records, reaches the same lowering over the same fragments. So
the header it writes matches the wrapper:

```c
double boltffi_function_geo_length(const uint8_t *line_ptr, uintptr_t line_len);
```

`#[data]`, `#[error]`, `#[export] trait`, `#[export] impl`, `#[custom_ffi]`,
`custom_type!` and `interned_string_pool!` define lanes. A declaration that refers to
itself calls its own lane, which already exists.

## Dependencies

A dependency's macros expand inside the dependency, so its wrappers are compiled into its
own rlib. Lanes cross crates through `#[macro_export]`, and records arrive with the
dependency's artifact. `boltffi generate` binds:

- the root crate and every path dependency it reaches through normal dependencies, in full;
- registry and git crates only for the declarations the bound ones reach.

Two bound crates that declare the same name are refused, since the foreign namespace is
flat. rustc links a dependency's rlib only when the root uses it. So after lowering,
`generate` checks every symbol the bindings call against the built library
(`boltffi_bindgen/src/library_symbols.rs`). A path dependency the root never names fails
there, and `use <crate> as _;` in the root fixes it.

## Identity

Declarations are identified by crate and name: `demo::Point`, `demo::Engine::start`.
Symbols follow, as in `boltffi_function_demo_add`. An invocation cannot know its module
path, and the foreign namespace is flat by name anyway. So two types or exports with the
same name in one crate fail to build: every lane also defines a crate-root
`__boltffi_declared_<Name>` macro, and every export a symbol.

Some generated items belong to one invocation and are used by another. These are reached
through the declaring type rather than by name:

- `ClassHandle` gives a class's handle type.
- `CallbackMarker` gives a callback trait's foreign proxy, through a marker value defined
  under the trait's name.
- `CallbackLocalHandle` registers a Rust implementation of a callback trait.

## Configuration

An attribute macro sees `#[cfg]` and `#[cfg_attr]` before rustc evaluates them. An item
that uses either defers to a derive, which sees its input after evaluation:

```rust
#[data]
pub struct Settings {
    pub name: String,
    #[cfg(feature = "metrics")] pub samples: u32,
}

// emitted instead of the lane chain
#[derive(::boltffi::__private::CfgEval)]
#[boltffi_cfg_eval(data {} { pub struct Settings { … } })]
enum __BoltffiCfgEval_data_Settings_0 {
    __BoltffiItem,
    F0,
    #[cfg(feature = "metrics")] F1,
}
```

The enum has one variant per member, and one per member `cfg_attr`. rustc strips the
variants whose `cfg` is off, so the derive knows exactly which members exist. It rebuilds
the item from them, then writes its record and starts its lane chain.

## Rules this imposes

- A `custom_type!` type is written by its declared name in signatures. The macro defines
  that name as an alias of the remote type.
- A class's `#[export] impl` sits in the same module as the struct, since its lane is
  defined there.
- A `type` alias names only the type, so a signature cannot name a declared type through
  one. `use … as` renames the type and its lane together.
- A `macro_rules!` with a declared type's name collides with, or shadows, its lane.
- A declared type cannot take a builtin's name (`Duration`, `SystemTime`, `Uuid`, `Url`).
  Every signature that spells such a name crosses it as the builtin.
- A parameter cannot carry `#[cfg]`, since the probe only covers members. Gate the whole
  function instead.

## Looking at a lane

`cargo expand` shows the lane definitions and wrappers. To see what a lane hands back,
call it with a callback that turns its input into a string:

```rust
macro_rules! show { ($($t:tt)*) => { const SHOWN: &str = stringify!($($t)*); } }
shapes::Line! { [show] {} }
```

To reproduce one invocation's expansion without its attribute, write the first expansion
by hand:

```rust
shapes::Line! { [::boltffi::__private::lane_resume] { export {} { pub fn length(line: Line) -> f64 { … } } [] [] } }
```

## Where the code lives

- `boltffi_macros/src/lane.rs`: chain start, resume, finish, and lane definitions.
- `boltffi_macros/src/capture.rs`: fragments and record emission.
- `boltffi_macros/src/cfg_eval.rs`: the configuration probe.
- `boltffi_binding`: `aggregate_records`, `aggregate_invocation`, `lower_invocation`,
  `reachable_from`, and the record format.
- `boltffi_bindgen/src/library_symbols.rs`: the check against the built library's exports.
- `boltffi_bindgen/src/artifact.rs`: reading records from built artifacts.

## Verification

`boltffi_bindgen` tests build fixture crates and compare the generated bindings with the
symbols the artifact exports. The fixtures cover:

- a crate naming its types through imports, renames, globs, nested paths, local scopes,
  and both custom-type forms;
- members that exist only under a feature, built with and without it;
- exports produced by `macro_rules!` and `include!`;
- types that refer to each other and to themselves;
- linked, unlinked and colliding dependencies.

Records lowered through this path match the legacy scanner declaration for declaration.
The demo passes on every platform target.
