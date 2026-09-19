# ADR: Binding IR. One Source of Truth Between Contracts and Backends

## Status
Proposed

## Authors
- [Ali Hilal](githhub.com/engali94)

## Date
2026-04-27

---

## 1. Background and Problem

### 1.1 The pipeline today

```
   user crate (Rust)
        |
        |  #[data], #[export], #[data(impl)] proc macros
        v
   boltffi_macros            (emits cfg-gated contract items)
        |
        |  emits per-target glue + parsed module info
        v
   boltffi_bindgen
        |   builds:
        |     FfiContract   (semantic side, in src/ir/contract.rs)
        |     AbiContract   (boundary side, in src/ir/abi.rs)
        v
   render/<lang>/             (one folder per target language)
        lower.rs   ->   plan.rs   ->   emit.rs / templates.rs
        |
        v
   generated source files (Swift, Kotlin, Java, JNI, C#, Dart,
                           TypeScript, Python, C)
```

Three stages produce one stream of files. The third stage exists nine times, once per target language, and is the part this ADR is about.

### 1.2 What the user writes

A real example from `examples/demo/src/records/blittable.rs`:

```rust
use boltffi::*;

/// A 2D point with double-precision coordinates.
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[data(impl)]
impl Point {
    pub fn new(x: f64, y: f64) -> Self          { Point { x, y } }
    pub fn origin()              -> Self          { Point { x: 0.0, y: 0.0 } }
    pub fn distance(&self)       -> f64           { (self.x * self.x + self.y * self.y).sqrt() }
    pub fn try_unit(x: f64, y: f64) -> Result<Self, String> { /* ... */ }
}

#[export]
pub fn make_point(x: f64, y: f64) -> Point { Point { x, y } }

#[export]
pub fn add_points(a: Point, b: Point) -> Point {
    Point { x: a.x + b.x, y: a.y + b.y }
}
```

The user expects this to produce a `Point` type, a `make_point` free function, and an `add_points` free function in every target language, all wired to the same Rust crate, all interoperating correctly.

### 1.3 What the bindgen produces from it

From `boltffi_bindgen/src/ir/contract.rs`, the semantic side:

```rust
pub struct FfiContract {
    pub package:   PackageInfo,
    pub catalog:   TypeCatalog,
    pub functions: Vec<FunctionDef>,
}

pub struct TypeCatalog {
    records:      IndexMap<RecordId,    RecordDef>,
    enums:        IndexMap<EnumId,      EnumDef>,
    callbacks:    IndexMap<CallbackId,  CallbackTraitDef>,
    custom_types: IndexMap<CustomTypeId, CustomTypeDef>,
    builtins:     IndexMap<BuiltinId,   BuiltinDef>,
    classes:      IndexMap<ClassId,     ClassDef>,
}
```

A `RecordDef` carries the user-visible shape (fields, constructors, methods, doc, deprecation) and exposes a method that classifies it:

```rust
impl RecordDef {
    pub fn is_blittable(&self) -> bool {
        let field_primitives: Vec<FieldPrimitive> = self.fields.iter()
            .filter_map(|f| match &f.type_expr {
                TypeExpr::Primitive(p) => Some(p.to_field_primitive()),
                _ => None,
            })
            .collect();
        let all_primitive = field_primitives.len() == self.fields.len();
        let classify_fields = if all_primitive { &field_primitives[..] } else { &[] };
        matches!(
            classification::classify_struct(self.is_repr_c, classify_fields),
            PassableCategory::Blittable,
        )
    }
}
```

So the FFI side already has an opinion, this record can cross by raw memory copy, or it cannot.

From `boltffi_bindgen/src/ir/abi.rs`, the boundary side:

```rust
pub struct AbiContract {
    pub package:    PackageInfo,
    pub calls:      Vec<AbiCall>,
    pub callbacks:  Vec<AbiCallbackInvocation>,
    pub streams:    Vec<AbiStream>,
    pub records:    Vec<AbiRecord>,
    pub enums:      Vec<AbiEnum>,
    pub free_buf:   Name<GlobalSymbol>,
    pub atomic_cas: Name<GlobalSymbol>,
}

pub struct AbiRecord {
    pub id:           RecordId,
    pub decode_ops:   ReadSeq,
    pub encode_ops:   WriteSeq,
    pub is_blittable: bool,
    pub size:         Option<usize>,
}

pub struct AbiEnum {
    pub id:                  EnumId,
    pub decode_ops:          ReadSeq,
    pub encode_ops:          WriteSeq,
    pub is_c_style:          bool,
    pub codec_tag_strategy:  EnumTagStrategy,
    pub variants:            Vec<AbiEnumVariant>,
}
```

Looking closely at `AbiRecord`. It carries:

- `is_blittable: bool`, saying "treat this as a direct memory copy".
- `decode_ops`/`encode_ops`, describing how to encode it as a wire buffer.

Both pieces of data are present at the same time, on every record, regardless of which path was actually decided. The struct is the union of "I am direct" and "I am encoded". The boolean is the only thing that picks between them. Everything downstream has to read the boolean and choose the matching field.

The same shape applies to `AbiEnum` (`is_c_style: bool` plus ops), to returns (`ReturnShape` carries optional ops plus a `ReturnContract` strategy enum), and to params (`ParamRole::Input` carries optional `decode_ops`, optional `encode_ops`, and a `ParamContract`).

### 1.4 What backends do with it

Each backend lives under `render/<lang>/` and follows the same triad:

- `lower.rs`. Walks `FfiContract` and `AbiContract`, produces a language-specific tree.
- `plan.rs`. Defines that language-specific tree.
- `emit.rs` / `templates.rs`. Turns the tree into source code.

There is no shared trait, no shared invariants, no shared helpers. Each backend implements the same pipeline shape from scratch.

Line counts (`find render/<lang> -name '*.rs' | xargs wc -l`) -comments included-:

| Backend     | Lines  |
|-------------|-------:|
| C#          | 10,210 |
| Java        |  9,189 |
| Kotlin      |  9,119 |
| Swift       |  8,117 |
| TypeScript  |  7,077 |
| JNI         |  5,192 |
| Python      |  4,425 |
| Dart        |  1,298 |
| C           |    890 |
| **Total**   | **55,517** |

`render/swift/lower.rs` alone is 3,523 lines. `render/jni/lower.rs` is 3,732 lines. `render/csharp/emit.rs` is 2,327 lines. None of these files share helpers, types, or invariants with the equivalent file in another backend. They share intent.

### 1.5 The drift in action

Take one fact about `Point`: is this record blittable? The codebase today has at least four independent answers.

**Answer 1.** The FFI catalog itself, on `RecordDef::is_blittable()` (shown in 1.3). Walks fields, checks `is_repr_c`, runs the classifier, returns a bool.

**Answer 2.** `AbiContract` lowering writes another bool into `AbiRecord::is_blittable` while building the boundary. A separate computation, in a different file.

**Answer 3.** Kotlin reaches into the ops sequence and re-derives blittability from the *shape* of the decode plan. From `render/kotlin/lower.rs`:

```rust
fn is_blittable_return(&self, ret_shape: &ReturnShape, returns_def: &ReturnDef) -> bool {
    if self.is_throwing_return(returns_def) {
        return false;
    }
    match &ret_shape.transport {
        Some(Transport::Span(SpanContent::Scalar(_)))    => true,
        Some(Transport::Span(SpanContent::Composite(_))) => true,
        _ => ret_shape.decode_ops
            .as_ref()
            .map(|ops| self.is_blittable_decode_seq(ops))
            .unwrap_or(false),
    }
}
```

This function does not call `AbiRecord::is_blittable`. It walks the `ReadSeq` and decides from the ops. Two answers, same question.

**Answer 4.** Swift skips both and queries the FFI catalog directly. From `render/swift/lower.rs`:

```rust
fn is_c_style_enum_return(&self, returns: &ReturnDef) -> bool {
    let enum_id = match returns {
        ReturnDef::Value(TypeExpr::Enum(id))
        | ReturnDef::Result { ok: TypeExpr::Enum(id), .. } => id,
        _ => return false,
    };
    self.contract.catalog.resolve_enum(enum_id)
        .map(|e| matches!(e.repr, EnumRepr::CStyle { .. }))
        .unwrap_or(false)
}

fn is_c_style_enum_type_expr(&self, type_expr: &TypeExpr) -> bool {
    let TypeExpr::Enum(enum_id) = type_expr else { return false; };
    self.contract.catalog.resolve_enum(enum_id)
        .map(|e| matches!(e.repr, EnumRepr::CStyle { .. }))
        .unwrap_or(false)
}
```

Swift reaches all the way back to `FfiContract`, ignoring `AbiEnum::is_c_style` entirely.

If `AbiContract` lowering computes the answer one way, Kotlin reads the ops another way, and Swift reads the FFI catalog a third way, the project has three predicates for one fact. Nothing in the type system lines them up. Any of the three can be wrong, the build keeps passing, and a runtime mismatch appears in user code. These four predicates are in the repository right now, and the symptom for `Point` is exactly the kind of bug that is invisible to compilation: one backend treats a `Point` return as a memcpy, another backend treats it as an encoded buffer, the runtime sends one shape, the binding decodes the other.

### 1.6 Why this happens, structurally

Three reasons. Each one is the shape of the data, not a discipline failure.

1. **`AbiRecord` carries both paths at once.** `is_blittable: bool`, `decode_ops`, `encode_ops`. The struct does not say "if blittable, you have a layout; if encoded, you have ops". It says "you have everything; the bool tells you which to use". A backend that ignores the bool, or computes its own bool, never trips a compile error. The data shape allows it.

2. **There is no common consumer trait.** Each backend defines its own consumer (the `lower.rs` struct), with its own methods, its own helpers, its own conventions. No `trait Backend` exists. Adding a new declaration kind in `FfiContract` does not break any backend at compile time. It becomes another match arm that backends silently ignore.

3. **Backends can reach upstream.** Swift and Kotlin lowering each hold a reference to `FfiContract` and walk it. A backend that wants a fact not in `AbiContract` fetches it from `FfiContract` and re-derives. There is no boundary saying "you only see the resolved binding".

Those three together are the reason adding the tenth backend, or changing how a record crosses, is a multi-thousand-line sweep with no compiler help.

### 1.7 What we do not have

The three reasons above point at three things missing from the architecture:

- A **single typed value** that represents the bindings the backends should emit, with the ABI decisions already made and *encoded structurally*. Different variants for different decisions, not flags layered on a union of all variants.
- A **fixed trait surface** that every backend implements, with the compiler enforcing exhaustiveness as the IR grows.
- A **crate boundary** that prevents backends from reaching upstream and re-deriving anything. Cargo enforces this, not convention: backend crates cannot name `boltffi_bindgen::ir` because that path is not in their dependency graph.

Until those three exist, drift is the path of least resistance. The architecture pushes backends toward redoing work, and toward disagreeing in subtle ways while doing it.

---

## 2. Goals

The redesign is judged against five criteria.

1. **The ABI decision is made once.** Whether a record is direct or encoded, whether an enum is C-style or data-bearing, where the async result lives. Backends consume the answer; they cannot redecide.
2. **Adding an IR concept breaks the build in every place that must handle it.** No silent defaults. No CI gap. The compiler is the checklist.
3. **Every backend handles the same set of declarations through the same shape of API.** Differences between languages live in templates and naming, not in semantics.
4. **Composition is cheap.** Adding a field, a variant, a codec leaf, or a declaration kind is additive and local.
5. **A new backend is bounded work.** Implementing a trait, a name style, a file grouping, and the leaves it needs. Not 8,000 lines of redecisions.

These are the criteria the alternatives are evaluated against.

---

## 3. Considered Alternatives

All three alternatives start from the same premise: produce one normalized binding tree upstream, feed it to backends, stop having backends re-derive ABI decisions. They differ in *how the consumer side is shaped* and *how forgetting is prevented*.

### 3.1 Alternative 1: Visitor Pattern

#### Shape

Define one tree of nodes. Define a `BackendVisitor` trait with one `visit_*` method per node, each with a default `walk_*` that recurses. Backends implement the trait and override what they need.

```rust
pub trait BackendVisitor {
    fn visit_module(&mut self, m: &Module) {
        walk_module(self, m);
    }
    fn visit_record(&mut self, r: &RecordDecl) {
        walk_record(self, r);
    }
    fn visit_enum(&mut self, e: &EnumDecl) {
        walk_enum(self, e);
    }
    fn visit_class(&mut self, c: &ClassDecl) {
        walk_class(self, c);
    }
    fn visit_function(&mut self, f: &FunctionDecl) {
        walk_function(self, f);
    }
    fn visit_callback(&mut self, c: &CallbackDecl) {
        walk_callback(self, c);
    }
    fn visit_stream(&mut self, s: &StreamDecl) {
        walk_stream(self, s);
    }
    fn visit_constant(&mut self, c: &ConstantDecl) {
        walk_constant(self, c);
    }
}

pub fn walk_module<V: BackendVisitor + ?Sized>(v: &mut V, m: &Module) {
    for r in &m.records   { v.visit_record(r); }
    for e in &m.enums     { v.visit_enum(e); }
    for c in &m.classes   { v.visit_class(c); }
    for f in &m.functions { v.visit_function(f); }
    for c in &m.callbacks { v.visit_callback(c); }
    for s in &m.streams   { v.visit_stream(s); }
    for k in &m.constants { v.visit_constant(k); }
}
```

A backend looks like this:

```rust
pub struct SwiftBackend {
    out: SwiftOutput,
}

impl BackendVisitor for SwiftBackend {
    fn visit_record(&mut self, r: &RecordDecl) {
        match r {
            RecordDecl::Direct(d)  => self.emit_swift_struct(d),
            RecordDecl::Encoded(e) => self.emit_swift_codable(e),
        }
    }
    fn visit_function(&mut self, f: &FunctionDecl) {
        self.emit_swift_function(f);
    }
    // visit_enum, visit_class, visit_callback, visit_stream, visit_constant
    // not overridden; default walk_* fires.
}
```

Driver:

```rust
let mut swift = SwiftBackend::new();
swift.visit_module(&bindings);
swift.out.into_files()
```

#### Why we considered it

The visitor pattern is a known shape. Engineers recognize it from compilers, AST processors, and IDE tooling. It separates traversal from per-node logic, and it is straightforward to add a second consumer (a linter, a JSON dumper, a documentation generator) that walks the same tree without touching backends.

#### Why we rejected it

**Defaults are silent.** Take the `SwiftBackend` above. It does not override `visit_callback`. A user crate later adds a callback trait:

```rust
#[callback]
pub trait ProgressReporter {
    fn on_progress(&self, percent: u32);
}
```

The bindgen produces a `Decl::Callback(...)` for it. The driver calls `swift.visit_callback(callback_decl)`. The default body fires `walk_callback`, which iterates the callback's methods and calls `swift.visit_function(...)` for each. Each method gets emitted as a free Swift function. The Swift output ends up with two free functions named after the trait's methods, no protocol declared, no vtable wired, no protocol witness table on the Rust side bound to a Swift class. The build passes. The Swift compiler accepts the file (the free functions are syntactically valid). Tests for record and class paths pass. The bug is only visible by reading the generated Swift and noticing the missing protocol. This is the exact category of failure the redesign is meant to eliminate.

The fix on paper is "remove the defaults and require every method". That moves the cost without changing the shape: adding a new `visit_resource_handle` to the trait still allows existing impls to compile until you remove their default. And once you require every method, you have arrived at Alternative 3 without the supporting pieces (private constructors, structural data shapes, crate boundary) that make the rest of the architecture work.

**One walker does not fit nine languages.** Swift emits value types, reference types, and extensions in three separate files, in a particular order, with imports at the top of each. Java emits a public class plus a JNI bridge in a different file, in a different package. C emits a header and an implementation. Kotlin emits one file per top-level declaration with shared imports.

A central `walk_module` is therefore one of:

1. The lowest common denominator, where each backend re-traverses inside its own state to actually produce files. The walker contributes nothing.
2. A growing set of hooks, like this:

```rust
pub trait BackendVisitor {
    fn module_header(&mut self, m: &Module)            { /* default no-op */ }
    fn pre_record(&mut self, r: &RecordDecl)           { /* default no-op */ }
    fn visit_record(&mut self, r: &RecordDecl)         { walk_record(self, r); }
    fn post_record(&mut self, r: &RecordDecl)          { /* default no-op */ }
    fn between_records(&mut self, prev: &RecordDecl,
                                  next: &RecordDecl)   { /* default no-op */ }
    fn module_footer(&mut self, m: &Module)            { /* default no-op */ }
    // ... and the equivalent for enums, classes, functions, callbacks
}
```

until the trait is itself a backend. We do not want either.

**The walker becomes a magnet for cross-cutting logic.** Any helper shared between backends has a natural home: inside `walk_*`. Today it is "skip this if hidden". Next quarter it is "if this record is direct on this platform but encoded on that platform, dispatch differently". The walker turns into the place ABI policy lives, which is the problem we are leaving behind.

**Adding a node is not enforced.** Adding `visit_resource_handle` adds a method to the trait with a default. Backends that have not been updated continue to compile. We have replaced one silent gap with another.

**Shared mutable state is awkward at scale.** Backends accumulate output (multiple file buffers, an imports set, deduplicated forward declarations). On `&mut self`, every helper lives on the visitor and every method coordinates with shared accumulators. With nine backends each owning a different accumulator shape, the trait either becomes generic over the accumulator (verbose) or every backend grows its own ad-hoc state machine (what we have today).

The visitor pattern is the right tool when the goal is *external traversals over a stable tree*. It is the wrong tool when the goal is *the tree forces every backend to handle every case*. We need the latter.

### 3.2 Alternative 2: Full Typestate DSL

#### Shape

Encode every obligation a binding declaration must satisfy as a phantom type parameter on its builder. Each obligation starts as `Missing` and transitions to `Decided` when the corresponding method is called. The terminal `build()` method exists only when every parameter is `Decided`.

```rust
pub struct Missing;
pub struct Decided;
pub trait Status {}
impl Status for Missing {}
impl Status for Decided {}

pub struct RecordScope<F, C, E, L, R, W, V, D, A>
where
    F: Status, C: Status, E: Status, L: Status,
    R: Status, W: Status, V: Status, D: Status, A: Status,
{
    inner: RecordInner,
    _markers: PhantomData<(F, C, E, L, R, W, V, D, A)>,
}

impl RecordScope<Missing, Missing, Missing, Missing,
                 Missing, Missing, Missing, Missing, Missing> {
    pub fn new(id: RecordId, name: CanonicalName) -> Self { /* ... */ }
}

impl<C, E, L, R, W, V, D, A> RecordScope<Missing, C, E, L, R, W, V, D, A>
where C: Status, E: Status, L: Status, R: Status, W: Status,
      V: Status, D: Status, A: Status,
{
    pub fn fields(self, fs: Vec<FieldDecl>)
        -> RecordScope<Decided, C, E, L, R, W, V, D, A> { /* ... */ }
}

impl<F, E, L, R, W, V, D, A> RecordScope<F, Missing, E, L, R, W, V, D, A>
where F: Status, E: Status, L: Status, R: Status, W: Status,
      V: Status, D: Status, A: Status,
{
    pub fn ctor(self, c: CtorDecl)
        -> RecordScope<F, Decided, E, L, R, W, V, D, A> { /* ... */ }
}

// ... seven more transition impls, one per obligation ...

impl RecordScope<Decided, Decided, Decided, Decided,
                 Decided, Decided, Decided, Decided, Decided> {
    pub fn build(self) -> RecordDecl { RecordDecl::from_inner(self.inner) }
}
```

Caller:

```rust
let record = RecordScope::new(point_id, canonical("Point"))
    .fields(vec![field("x", F64), field("y", F64)])
    .ctor(ctor_default())
    .equality(Equality::Structural)
    .lifecycle(Lifecycle::ValueSemantics)
    .read(read_plan)
    .write(write_plan)
    .visibility(Visibility::Public)
    .docs(docs("A 2D point"))
    .attrs(attrs())
    .build();
```

Forgetting any one method is a compile error: `build()` does not exist on the resulting type.

#### Why we considered it

Maximum static guarantees. Every obligation tracked. Forgetting one is impossible. All enforcement in the type. No runtime validation. Errors are local to the call site. This is precisely the kind of thing Rust's type system is designed for. We seriously considered it. It was the most appealing of the three.

#### Why we rejected it

**Phantom-parameter explosion.** Records have around nine obligations (fields, ctor, equality, lifecycle, read, write, visibility, docs, attrs). Functions have at least as many (params, receiver, returns, error transport, sync/async, callback shape, native symbol, visibility, docs). Classes have more (constructors, methods, statics, ownership, drop semantics, plus the class-level obligations). Every scope ends up with eight to twelve phantom parameters.

Wherever these scopes flow, the parameters flow with them. A helper that adds a default constructor to records that do not already specify one:

```rust
fn add_default_ctor<F, C, E, L, R, W, V, D, A>(
    scope: RecordScope<F, C, E, L, R, W, V, D, A>,
) -> RecordScope<F, Decided, E, L, R, W, V, D, A>
where F: Status, C: Status, E: Status, L: Status, R: Status,
      W: Status, V: Status, D: Status, A: Status,
{
    scope.ctor(default_ctor())
}
```

Every helper signature looks like that. Every method that takes a partial scope looks like that. Even helpers that do not care about most parameters have to name and bound them, because the type is parameterized over them.

**Composition fights the typestate.** Real DSL usage wants to put partially built scopes into a `Vec`, pass them through helpers, share construction across two related declarations, build them inside loops. All of those break when each scope is a different type:

```rust
// Resolver wants to build a Vec of partially-configured records,
// then finish them in a second pass once cross-record analysis is done.
let mut partial: Vec<???> = Vec::new();         // What goes here?

for record_def in records {
    let stage1 = RecordScope::new(record_def.id, record_def.name)
        .fields(record_def.fields.clone());
    partial.push(stage1);                       // Type after .fields() is
                                                // RecordScope<Decided, Missing,
                                                //             Missing, Missing,
                                                //             Missing, Missing,
                                                //             Missing, Missing,
                                                //             Missing>.
}

for record_def in records {
    let stage1 = RecordScope::new(record_def.id, record_def.name);
    partial.push(stage1);                       // Different type:
                                                // RecordScope<Missing, ..., Missing>.
                                                // Cannot share Vec with the previous
                                                // loop's elements.
}
```

`Vec` requires one element type. Two scopes at different stages of construction are different types. The compiler cannot store them together.

The workaround is `Box<dyn AnyScope>`, an erased trait object that hides the phantom parameters. As soon as you erase, the static guarantee is gone. `build()` becomes a runtime call that returns `Result<RecordDecl, MissingObligationError>`. The typestate has just been collapsed to a runtime check at the boundary, exactly where sharing would have been useful.

**Error messages stop being readable.** A typical mistake produces:

```
error[E0599]: no method named `build` found for struct
  `RecordScope<Decided, Decided, Missing, Decided, Decided,
               Decided, Decided, Decided, Decided>`
  in the current scope
   --> binding/src/compose/record.rs:142:18
    |
142 |     let r = scope.build();
    |                   ^^^^^ method not found in
    |             `RecordScope<Decided, Decided, Missing, Decided, Decided,
    |                          Decided, Decided, Decided, Decided>`
```

The compiler is correct. But nobody can read it. Reviewers cannot see at a glance which slot is `Missing`. It is the third one, which corresponds to `equality`. The message says nothing about that. We would have to write tooling to translate type errors back into English ("you forgot to call `.equality()` on this record"). That tooling is itself a language.

**Evolution is a cascade.** Adding a tenth obligation to `RecordScope` (say `serde_strategy`) means adding a tenth phantom parameter. Every transition impl gets a new generic parameter. Every helper signature that mentions `RecordScope` gets a new generic parameter. Every `where` bound expands. A change that should be local becomes a sweep through every signature in the DSL.

**It enforces the wrong thing.** The drift in Section 1.5 is *cross-backend*. Swift saying a record is C-style. Kotlin saying it is encoded. The two disagreeing. Typestate enforces *per-builder* obligations during construction inside `binding/`. That is useful for the small set of obligations that are silently forgettable. It is not the hard problem. The hard problem is the boundary between `binding/` and the backends, and the cardinality of the `Decl` enum. Typestate addresses neither.

**The macros that hide it become the architecture.** Realistically, callers do not write phantom-typestate by hand. They write a macro:

```rust
record_scope! {
    id:         point_id,
    name:       "Point",
    fields:     [field("x", F64), field("y", F64)],
    ctor:       ctor_default(),
    equality:   Equality::Structural,
    lifecycle:  Lifecycle::ValueSemantics,
    read:       read_plan,
    write:      write_plan,
    visibility: Visibility::Public,
    docs:       "A 2D point",
    attrs:      [],
}
```

The macro expands to the typestate chain. The typestate is a private implementation detail. The architecture is the macro plus what it produces. The typestate underneath costs compile time, error-message clarity, and signature noise without earning its keep, because the macro is what callers actually interact with.

Full typestate is the right tool when the obligations are a small fixed set, the builder is used in one place, and helpers do not need to flow partially built scopes around. Our case is the opposite on all three.

### 3.3 Alternative 3 (Accepted): Binding IR with Sealed Decls and Targeted Typestate

#### Shape

A new crate `binding/` produces one value, `Bindings`, from `FfiContract + AbiContract`. Backends consume `Bindings` only. They do not see `FfiContract`, `AbiContract`, raw codec ops, or any of the upstream types. The crate boundary is the lock.

Public surface of `binding/`:

```rust
// binding/src/lib.rs

pub struct Bindings {
    package: PackageInfo,
    decls:   Vec<Decl>,
    names:   NameTable,
    symbols: NativeSymbolManifest,
    caps:    Capabilities,
}

impl Bindings {
    pub fn decls(&self)   -> &[Decl]                { &self.decls }
    pub fn names(&self)   -> &NameTable             { &self.names }
    pub fn symbols(&self) -> &NativeSymbolManifest  { &self.symbols }
    pub fn caps(&self)    -> Capabilities           { self.caps }
}

#[non_exhaustive]
pub enum Decl {
    Record(RecordDecl),
    Enum(EnumDecl),
    Class(ClassDecl),
    Function(FunctionDecl),
    Callback(CallbackDecl),
    Stream(StreamDecl),
    Constant(ConstantDecl),
}
```

Four properties of this module are the actual architecture. Each one corresponds to one of the three structural reasons drift happens today (Section 1.6).

#### (a) Crossing decisions encoded as separate variants with different data shapes

This is the answer to "AbiRecord carries both paths at once". `RecordDecl` is split:

```rust
pub enum RecordDecl {
    Direct(DirectRecord),
    Encoded(EncodedRecord),
}

pub struct DirectRecord {
    meta:   DeclMeta,
    name:   CanonicalName,
    fields: Vec<FieldDecl>,
    layout: DirectLayout,        // size, alignment, field offsets
                                 // no codec, no ops
}

pub struct EncodedRecord {
    meta:   DeclMeta,
    name:   CanonicalName,
    fields: Vec<FieldDecl>,
    codec:  CodecNode,           // read and write composition
                                 // no layout
}

pub struct FieldDecl {
    pub name: CanonicalName,     // role = NameRole::Field
    pub ty:   TypeRef,
    pub doc:  Option<String>,
}
```

`FieldDecl` is the same shape in both variants. The structural difference between Direct and Encoded is what *accompanies* the field list (`layout` versus `codec`), not the fields themselves. The codec, when present, is a `CodecNode` tree whose composer arms reuse the same `CanonicalName` for field names so that "the field declared on the type" and "the field encoded on the wire" cannot drift in spelling. A single shape across the IR.

The `is_blittable: bool` flag is gone. `DirectRecord` does not have a `codec`. `EncodedRecord` does not have a `layout`. A backend cannot accidentally encode a direct record because there is no codec to encode with. The field is not in scope.

Same structural split for enums, async results, error transport, handles, and stream delivery:

```rust
pub enum EnumDecl {
    CStyle(CStyleEnum),               // tag type, variants, no payload codec
    Data(DataEnum),                   // tagged union with per-variant codec
}

pub enum AsyncResult {
    NativeAwait(NativeAwaitShape),    // poll, complete, free symbols
    Continuation(ContinuationShape),  // continuation pointer ABI
    PromiseLike(PromiseShape),        // JS-style fulfill/reject
}

pub enum ErrorTransport {
    None,
    StatusCode(StatusCodeShape),
    Encoded(EncodedErrorShape),       // carries decode codec only
}
```

In every case the rule is the same: decide upstream, encode the decision as a variant, do not attach optional fields that allow the backend to revisit it.

#### (b) Sealed `Decl` with private constructors and an exhaustive backend trait

This is the answer to "no common consumer trait, backends silently diverge as the IR grows". `Decl` is `#[non_exhaustive]`. The inner types have private fields and no public constructors. Every variant is built only inside `binding/` by the resolver.

Backends implement one method per variant:

```rust
// binding/src/backend.rs

pub trait Backend {
    /// The set of capabilities this backend can emit code for. The driver
    /// rejects bindings that need a flag the backend has not opted into.
    fn supports(&self) -> Capabilities;

    fn render_record(&mut self,   d: &RecordDecl)   -> Emitted;
    fn render_enum(&mut self,     d: &EnumDecl)     -> Emitted;
    fn render_class(&mut self,    d: &ClassDecl)    -> Emitted;
    fn render_function(&mut self, d: &FunctionDecl) -> Emitted;
    fn render_callback(&mut self, d: &CallbackDecl) -> Emitted;
    fn render_stream(&mut self,   d: &StreamDecl)   -> Emitted;
    fn render_constant(&mut self, d: &ConstantDecl) -> Emitted;
}

pub struct Emitted {
    pub primary:     Source,
    pub aux:         Vec<AuxChunk>,        // imports, forward decls, helpers
    pub diagnostics: Vec<Diagnostic>,
}

pub enum DriveError {
    CapabilityMismatch {
        required: Capabilities,
        supported: Capabilities,
        missing:  Capabilities,
    },
}

pub fn drive<B: Backend>(b: &mut B, bindings: &Bindings) -> Result<Vec<Emitted>, DriveError> {
    // Capability gate. The bindings advertise what the generated code needs
    // (async strategy, error transport, handle kind). The backend advertises
    // what it can render. `required ⊆ supported` is the only acceptable
    // shape; otherwise we fail fast with a precise diagnostic.
    let required  = bindings.caps();
    let supported = b.supports();
    if !supported.contains(required) {
        return Err(DriveError::CapabilityMismatch {
            required,
            supported,
            missing: required - supported,
        });
    }

    Ok(bindings.decls().iter().map(|decl| match decl {
        Decl::Record(d)   => b.render_record(d),
        Decl::Enum(d)     => b.render_enum(d),
        Decl::Class(d)    => b.render_class(d),
        Decl::Function(d) => b.render_function(d),
        Decl::Callback(d) => b.render_callback(d),
        Decl::Stream(d)   => b.render_stream(d),
        Decl::Constant(d) => b.render_constant(d),
    }).collect())
}
```

The match is exhaustive. The trait has no defaults. Adding a variant breaks every `impl Backend` and breaks `drive` until updated. There is no place to forget.

A backend's record handler fans out on the structural split:

```rust
impl Backend for SwiftBackend {
    fn supports(&self) -> Capabilities {
        Capabilities::ASYNC_NATIVE_AWAIT
            | Capabilities::ERROR_RESULT_TAG
            | Capabilities::HANDLE_OPAQUE
            | Capabilities::CALLBACK_VTABLE
    }

    fn render_record(&mut self, d: &RecordDecl) -> Emitted {
        match d {
            RecordDecl::Direct(r)  => self.render_direct_record(r),
            RecordDecl::Encoded(r) => self.render_encoded_record(r),
        }
    }
    // ... six more render_* methods, one per Decl variant
}

impl SwiftBackend {
    fn render_direct_record(&mut self, r: &DirectRecord) -> Emitted {
        // Has r.layout. Emits a Swift struct with @frozen and stored fields,
        // an init from a fixed-size byte buffer using r.layout.field_offsets,
        // Equatable and Hashable conformances from r.meta.equality.
        // Cannot use a codec. There isn't one.
    }

    fn render_encoded_record(&mut self, r: &EncodedRecord) -> Emitted {
        // Has r.codec. Emits a Swift struct plus encode(into:)/decode(from:)
        // by walking r.codec with the shared CodecRead/CodecWrite walkers.
        // Cannot do a memcpy. There is no layout.
    }
}
```

#### (c) The crate boundary blocks reach-through

This is the answer to "backends can reach upstream and re-derive". `binding/` is a crate. Backend crates depend on `binding/` only. They do not depend on `boltffi_bindgen::ir`, do not see `FfiContract`, do not see `AbiContract`. The Swift code that today does this:

```rust
// today, in render/swift/lower.rs
self.contract.catalog.resolve_enum(enum_id)
    .map(|e| matches!(e.repr, EnumRepr::CStyle { .. }))
```

cannot exist after the redesign. The symbols `contract`, `catalog`, `resolve_enum`, `EnumRepr` are not in scope in a backend crate. Re-derivation is not a discipline question. It is a build-system question.

#### (d) Targeted typestate, behind macros, on the silently forgettable obligations

This is where Alternative 2's idea is preserved exactly where it earns its keep, and dropped everywhere else.

A small set of obligations on a few declarations are silently wrong if forgotten. For records: constructor decision, equality decision, lifecycle decision. Forgetting any of these in the resolver would build a `RecordDecl` whose backend rendering is meaningless. Those obligations get phantom-type tracking. Everything else (docs, attrs, visibility, fields) is plain.

The typestate lives behind a macro so callers never see it:

```rust
// binding/src/compose.rs (private to the crate)
//
// sealed_decl! generates the inner type, accessor methods, and HasMeta impl.
// It also generates a typestate scope with phantom params only for the
// obligations listed in `required`.
sealed_decl! {
    decl:     RecordDecl,
    inner:    RecordInner,
    meta:     DeclMeta,
    optional: [docs, attrs, visibility, fields],
    required: [ctor, equality, lifecycle],
}

// The user-facing macro hides everything.
bind_record! { id: point_id, name: "Point", |s| {
    s.field("x", F64)
     .field("y", F64)
     .ctor(ctor_default())
     .equality(Equality::Structural)
     .lifecycle(Lifecycle::ValueSemantics)
     .docs("A 2D point")
}};
```

If the resolver writes:

```rust
bind_record! { id: point_id, name: "Point", |s| {
    s.field("x", F64).field("y", F64)
     .ctor(ctor_default())
     // forgot .equality
     .lifecycle(Lifecycle::ValueSemantics)
}};
```

the macro expansion produces a labeled compile error pointing at the closure with a message like "missing required obligation: equality". Phantom-type machinery is in the macro expansion only. Callers see plain method chains. Helpers and reuse are not parameterized over phantom params, because the typestate scope is closure-local: it lives and dies inside the `|s|` block.

The hard rule that keeps the macro footprint small: **reusable resolver helpers do not traffic in partially-built scopes**. They take fully-built `Decl` values, or they take primitive parameters and build the scope locally inside their own `bind_record!` block. The typestate scope is closure-local by construction. Whatever DSL is exposed publicly stays narrow: a closure-shaped builder for the few declarations with silently forgettable obligations, nothing more. Crossing that line (exposing partial scopes as a function argument or a return type) reintroduces the phantom-parameter explosion that Alternative 2 was rejected for, and is what the macro is shaped to prevent.

#### The resolver

The resolver is the only place that builds `Bindings`. It is a pure function from upstream contracts to a `Bindings` value:

```rust
// binding/src/resolve.rs

pub fn resolve(ffi: &FfiContract, abi: &AbiContract) -> Result<Bindings, ResolveError> {
    let mut names   = NameTable::new();
    let mut symbols = NativeSymbolManifest::new();

    let record_decls = ffi.catalog.all_records()
        .map(|record| {
            let abi_record = abi.records.iter()
                .find(|r| r.id == record.id)
                .ok_or_else(|| ResolveError::MissingAbiRecord(record.id.clone()))?;
            // Single decision site. After this branch the answer is structural.
            let decl = match abi_record.is_blittable {
                true  => bind_record_direct(record, abi_record, &mut names)?,
                false => bind_record_encoded(record, abi_record, &mut names)?,
            };
            Ok::<_, ResolveError>(Decl::Record(decl))
        });

    let enum_decls = ffi.catalog.all_enums()
        .map(|enum_def| {
            let abi_enum = abi.enums.iter()
                .find(|e| e.id == enum_def.id)
                .ok_or_else(|| ResolveError::MissingAbiEnum(enum_def.id.clone()))?;
            let decl = match &enum_def.repr {
                EnumRepr::CStyle { tag_type } => bind_enum_cstyle(enum_def, abi_enum, *tag_type)?,
                EnumRepr::Data    { .. }      => bind_enum_data(enum_def, abi_enum)?,
            };
            Ok::<_, ResolveError>(Decl::Enum(decl))
        });

    // ... and equivalent chains for classes, functions, callbacks, streams, constants.

    let decls: Vec<Decl> = record_decls
        .chain(enum_decls)
        // .chain(class_decls).chain(function_decls)...
        .collect::<Result<_, _>>()?;

    let caps = derive_capabilities(ffi, abi);
    // from_resolved_parts validates internally and returns the only
    // public path to a Bindings value. There is no unchecked constructor.
    Bindings::from_resolved_parts(ffi.package.clone(), decls, names, symbols, caps)
}
```

The branch `if abi_record.is_blittable { Direct } else { Encoded }` is the only place the question is ever asked. After that, the answer is structural. The branch happens once, in one file.

#### `Bindings::validate()` and the cross-Decl invariants

`Bindings::from_resolved_parts` calls `validate()` internally. There is no public way to construct an unvalidated `Bindings`. `validate()` enforces invariants that span declarations, where a single-Decl type cannot. A few examples:

```rust
impl Bindings {
    pub(crate) fn validate(&self) -> Result<(), ResolveError> {
        // Every SymbolId referenced by any Decl resolves to a NativeSymbol
        // in the manifest. SymbolIds in the manifest are unique. Multiple
        // Decls may reference the same SymbolId, and multiple SymbolIds may
        // resolve to the same runtime symbol string (a thin wrapper and its
        // target both calling one helper is a valid shape).
        self.check_symbol_references_resolve()?;

        // Every entry in the manifest is referenced by at least one Decl.
        // Catches orphan exports left in the manifest after a Decl is removed.
        self.check_no_orphan_symbols()?;

        // No two Decls produce the same canonical name in the same namespace.
        self.check_canonical_name_uniqueness()?;

        // Every CallbackId referenced by a FunctionDecl::params or by a
        // ClassDecl::methods exists as a CallbackDecl in the same Bindings.
        self.check_callback_references_resolve()?;

        // No unguarded reference cycles between record decls. (Cycles via
        // Box, Option, or Vec are fine; direct recursive composition is not.)
        self.check_no_unguarded_cycles()?;

        // For every LowerPlan::DirectRecord(id), the corresponding RecordDecl
        // is RecordDecl::Direct. Mismatches between plan and decl are rejected.
        self.check_plan_decl_consistency()?;

        // Every Capabilities flag set has at least one Decl that requires it.
        // This catches over-claimed capabilities silently inflating the runtime.
        self.check_capabilities_are_used()?;

        Ok(())
    }
}
```

Invalid `Bindings` cannot reach a backend. A backend can therefore assume every invariant in this list holds, and stop defensive-coding for the case where it does not.

#### Worked example: `Point` end to end

Today (drift surface), the `Point` example from Section 1.2 produces an `AbiRecord` like this:

```rust
// what every backend receives today
AbiRecord {
    id:           RecordId("Point"),
    decode_ops:   ReadSeq { ops: vec![ReadOp::Record { id: ..., fields: ... }] },
    encode_ops:   WriteSeq { ops: vec![WriteOp::Record { id: ..., fields: ... }] },
    is_blittable: true,
    size:         Some(16),
}
```

Both encoded ops and the blittable-with-size path are present. Each backend then makes its own call. Swift's `is_c_style_enum_return` style lookups, Kotlin's `is_blittable_return`, the `AbiRecord::is_blittable` boolean. Three predicates can disagree. The bug surface is here.

After the redesign, the resolver branches once and the backends receive structurally distinct data:

```rust
// what every backend receives after the redesign.
// Canonical names are stored snake_case; the backend's NameStyle
// renders "point" as Swift "Point", Python "point", and so on.
Decl::Record(RecordDecl::Direct(DirectRecord {
    meta:   DeclMeta { docs: Some("A 2D point"), attrs: [...], visibility: Public, ... },
    name:   CanonicalName::parse_type("point")?,
    fields: vec![
        FieldDecl { name: CanonicalName::parse_field("x")?, ty: F64 },
        FieldDecl { name: CanonicalName::parse_field("y")?, ty: F64 },
    ],
    layout: DirectLayout {
        size:           16,
        alignment:      8,
        field_offsets:  vec![("x", 0), ("y", 8)],
    },
}))
```

There is no `encode_ops`. There is no `is_blittable: bool`. The Swift backend's `render_direct_record` receives this value, has `r.layout.field_offsets` to use, and physically cannot reach for a codec. The Kotlin backend's `render_direct_record` receives the same value and cannot disagree about blittability because there is nothing to disagree with. Consistency is structural.

For a non-blittable record (say, one with a `String` field), the same record arrives as:

```rust
Decl::Record(RecordDecl::Encoded(EncodedRecord {
    meta:   ...,
    name:   CanonicalName::parse_type("user_profile")?,
    fields: vec![
        FieldDecl { name: CanonicalName::parse_field("name")?, ty: String },
        FieldDecl { name: CanonicalName::parse_field("age")?,  ty: U32 },
    ],
    codec:  CodecNode::Struct(vec![
        NamedCodec { name: CanonicalName::parse_field("name")?, node: CodecNode::Leaf(LeafCodec::String) },
        NamedCodec { name: CanonicalName::parse_field("age")?,  node: CodecNode::Leaf(LeafCodec::U32) },
    ]),
}))
```

No layout. No memcpy path available. Both backends walk the codec with the shared `walk_read` / `walk_write` walkers and emit equivalent decode logic. Structural equality of the tree means equivalent generated code.

#### Supporting pieces

The four properties above (variant split, sealed Decl, crate boundary, targeted typestate) are the architectural moves. The pieces below are what makes those moves work in practice. Each one lives in `binding/` and each one solves a specific recurring problem in the current backends.

**Codec as a tree of `CodecNode`.** Encoding and decoding are decomposed into leaves and composers. Backends implement only the leaves; composition is shared.

```rust
pub enum CodecNode {
    Leaf(LeafCodec),                       // Bool, U32, F64, String, Bytes, Handle
    Sequence(Box<CodecNode>),              // Vec<T>, repeated fields
    Optional(Box<CodecNode>),              // Option<T>
    Struct(Vec<NamedCodec>),               // record fields in declaration order
    Tagged { tag: TagShape, arms: Vec<TaggedArm> },   // tagged unions / data enums
}

pub struct NamedCodec {
    pub name: CanonicalName,               // role = NameRole::Field; matches FieldDecl::name
    pub node: CodecNode,
}

pub struct TaggedArm {
    pub name:        CanonicalName,        // role = NameRole::Variant
    pub discriminant: i128,
    pub node:        CodecNode,            // payload codec, possibly CodecNode::Struct(vec![])
}

pub trait CodecRead  { fn read(&self,  ctx: &mut ReadCtx)  -> ReadOut;  }
pub trait CodecWrite { fn write(&self, ctx: &mut WriteCtx) -> WriteOut; }

pub fn walk_read<L: CodecRead>(
    node: &CodecNode, leaves: &L, ctx: &mut ReadCtx,
) -> ReadOut {
    match node {
        CodecNode::Leaf(l)        => leaves.read(l, ctx),
        CodecNode::Sequence(inner) => ctx.with_sequence(|ctx| walk_read(inner, leaves, ctx)),
        CodecNode::Optional(inner) => ctx.with_optional(|ctx| walk_read(inner, leaves, ctx)),
        CodecNode::Struct(fields)  => ctx.with_struct(|ctx| {
            fields.iter().map(|f| (f.name.clone(), walk_read(&f.node, leaves, ctx))).collect()
        }),
        CodecNode::Tagged { tag, arms } => ctx.with_tagged(tag, arms, |ctx, arm| {
            walk_read(&arm.node, leaves, ctx)
        }),
    }
}
```

A new backend implements `CodecRead`/`CodecWrite` for its primitive read and write only (six leaves: bool, ints, floats, string, bytes, handle). Walking a record or a tagged union is the same code in every backend, because the walker is shared. The "Kotlin re-derives blittability from the shape of the decode plan" failure mode (Section 1.5, Answer 3) cannot recur, because the backend never receives "the shape of the decode plan" as a thing to walk for ABI decisions; it walks it only to emit code, and only via the shared walker.

**`Op<T>` typed expression algebra.** Backends emit small expressions all over the place: "size of this type in bytes", "number of elements in this vec", "is this option non-null", "the value of this scalar field". Today these are mixed strings or untyped enums, and the bug pattern is mixing a byte size with an element count. The new IR uses a phantom-typed expression so the compiler refuses to mix them.

```rust
pub struct Op<Kind> {
    expr:    OpExpr,
    _marker: PhantomData<Kind>,
}

pub enum OpExpr {
    Const(i64),
    Field(FieldId),
    SizeOf(TypeRef),
    LengthOf(FieldId),
    Add(Box<OpExpr>, Box<OpExpr>),
    Mul(Box<OpExpr>, Box<OpExpr>),
    IsSome(FieldId),
    Eq(Box<OpExpr>, Box<OpExpr>),
}

// Phantom kinds. Empty types that label what an Op is computing.
pub struct ByteCount;
pub struct ElementCount;
pub struct Truth;
pub struct Scalar<T>(PhantomData<T>);

impl Op<ByteCount> {
    pub fn size_of(t: TypeRef)               -> Self                { /* ... */ }
    pub fn add(self, rhs: Op<ByteCount>)     -> Op<ByteCount>       { /* ... */ }
    // No `add` between Op<ByteCount> and Op<ElementCount>. The impl does not exist.
}

impl Op<ElementCount> {
    pub fn length_of(f: FieldId)             -> Self                { /* ... */ }
}

// Backends render leaves only.
pub trait OpRender {
    fn render_const(&self, v: i64)            -> String;
    fn render_field(&self, f: &FieldId)       -> String;
    fn render_size_of(&self, t: &TypeRef)     -> String;
    fn render_length_of(&self, f: &FieldId)   -> String;
    fn render_add(&self, a: &str, b: &str)    -> String;
    // ...
}

pub fn render_op<R: OpRender, K>(r: &R, op: &Op<K>) -> String { /* recursion over OpExpr */ }
```

Adding a `Op<ByteCount>` to an `Op<ElementCount>` is not a runtime check that the resolver might forget. It is a missing trait impl that the compiler reports at the call site.

**`CanonicalName` plus per-backend `NameStyle`.** Names cross language boundaries with three concerns the backends keep getting wrong: casing convention, reserved-word collision, and identifier escaping. Today each backend handles these inline, with its own helpers, with its own list of keywords. The IR carries a single `CanonicalName` and per-backend policy lives in one trait.

```rust
pub struct CanonicalName {
    parts: Vec<NameSegment>,         // ["user", "profile"], ["xml", "parser"]
    role:  NameRole,                 // Type, Function, Field, Variant, Module
}

impl CanonicalName {
    /// Parse a snake_case canonical name with an explicit role.
    /// "user_profile" -> NameSegment ["user", "profile"]. The role decides
    /// how a backend's NameStyle renders it (PascalCase for Type, camelCase
    /// for Function and Field, and so on). Canonical storage is always
    /// snake_case; rendering is the backend's job.
    pub fn parse(role: NameRole, snake: &str) -> Result<Self, NameError> { /* ... */ }

    pub fn parse_type(snake:     &str) -> Result<Self, NameError> { Self::parse(NameRole::Type,     snake) }
    pub fn parse_function(snake: &str) -> Result<Self, NameError> { Self::parse(NameRole::Function, snake) }
    pub fn parse_field(snake:    &str) -> Result<Self, NameError> { Self::parse(NameRole::Field,    snake) }
    pub fn parse_variant(snake:  &str) -> Result<Self, NameError> { Self::parse(NameRole::Variant,  snake) }
    pub fn parse_module(snake:   &str) -> Result<Self, NameError> { Self::parse(NameRole::Module,   snake) }

    pub fn parts(&self) -> &[NameSegment] { &self.parts }
    pub fn role(&self)  -> NameRole       { self.role }
}

pub trait NameStyle {
    fn type_name(&self,     n: &CanonicalName) -> String;
    fn function_name(&self, n: &CanonicalName) -> String;
    fn field_name(&self,    n: &CanonicalName) -> String;
    fn variant_name(&self,  n: &CanonicalName) -> String;
    fn module_name(&self,   n: &CanonicalName) -> String;
    fn escape_reserved(&self, name: String) -> String;
}

pub struct SwiftNameStyle;
impl NameStyle for SwiftNameStyle {
    fn type_name(&self, n: &CanonicalName) -> String {
        self.escape_reserved(pascal_case(n.parts()))
    }
    fn function_name(&self, n: &CanonicalName) -> String {
        self.escape_reserved(camel_case(n.parts()))
    }
    fn field_name(&self, n: &CanonicalName) -> String {
        self.escape_reserved(camel_case(n.parts()))
    }
    fn escape_reserved(&self, name: String) -> String {
        if SWIFT_KEYWORDS.binary_search(&name.as_str()).is_ok() {
            format!("`{}`", name)
        } else {
            name
        }
    }
    // ...
}
```

A backend cannot emit a name as a string without going through its `NameStyle`. There is no `format!("{}", name.as_str())` shortcut, because `CanonicalName` does not implement `Display`. Casing, escaping, and keyword handling stop being per-call-site decisions.

**`NativeSymbol` validated identifiers.** Today, a typo in a `extern "C"` symbol name in a backend produces a binding that links and runs and explodes at the first call. The fix is twofold: validate symbol strings on construction, and verify the backend's symbol set against the runtime's actual exports at bindgen build time.

```rust
pub struct NativeSymbol(String);

impl NativeSymbol {
    pub fn new(s: impl Into<String>) -> Result<Self, NameError> {
        let s = s.into();
        let mut chars = s.chars();
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return Err(NameError::InvalidNativeSymbol(s)),
        }
        if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(NameError::InvalidNativeSymbol(s));
        }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

pub struct NativeSymbolManifest {
    by_id:    IndexMap<SymbolId, NativeSymbol>,
    free_buf: NativeSymbol,
    poll:     Option<NativeSymbol>,
    cas:      Option<NativeSymbol>,
}

impl NativeSymbolManifest {
    pub fn verify_against_runtime(&self, runtime: &RuntimeExports) -> Result<(), VerifyError> {
        for (_, sym) in &self.by_id {
            if !runtime.exports.contains(sym.as_str()) {
                return Err(VerifyError::SymbolNotExported(sym.clone()));
            }
        }
        for exported in &runtime.exports {
            if !self.by_id.values().any(|s| s.as_str() == exported) {
                return Err(VerifyError::ExportNotInManifest(exported.clone()));
            }
        }
        Ok(())
    }
}
```

Backends look up symbols by `SymbolId`, never by typing strings. The class of bug "I renamed the runtime function and one of nine backends still spells the old name" is replaced by a bindgen build failure at the manifest verification step.

**`ReceiverDecl` separate from `ParamDecl`.** Methods have a receiver (`self`, `&self`, `&mut self`, owned class handle). Some backends today carry the receiver as `params[0]` and skip it; others carry it as a separate field; the inconsistency is the source of a recurring filter bug where the receiver leaks into the user-visible parameter list. The IR makes them different fields with different types.

```rust
pub enum ReceiverDecl {
    None,                                    // free function or static method
    Borrowed,                                // &self
    BorrowedMut,                             // &mut self
    Owned,                                   // self (consuming)
    Handle { kind: HandleKind },             // class instance via opaque handle
}

pub struct FunctionDecl {
    pub meta:     DeclMeta,
    pub name:     CanonicalName,
    pub symbol:   SymbolId,
    pub receiver: ReceiverDecl,
    pub params:   Vec<ParamDecl>,            // never includes the receiver
    pub returns:  ReturnDecl,
    pub form:     CallableForm,
}
```

A backend iterating `params` sees only the user-visible parameters. It iterates `receiver` explicitly when binding a method call. There is no "skip the first one if this is a method" logic to forget.

**Capabilities as shape-encoded bitflags.** A `Bindings` value advertises what features the generated code needs, so backends emit only the imports, runtime hooks, and helper modules that are actually required. Today this is per-backend ad-hoc scanning of decls; in the IR it is one query.

```rust
bitflags! {
    pub struct Capabilities: u32 {
        const ASYNC_NATIVE_AWAIT          = 1 <<  0;
        const ASYNC_CONTINUATION_POINTER  = 1 <<  1;
        const ASYNC_PROMISE_LIKE          = 1 <<  2;
        const ERROR_RESULT_TAG            = 1 <<  3;
        const ERROR_STATUS_CODE           = 1 <<  4;
        const HANDLE_OPAQUE               = 1 <<  5;
        const STREAM_BACKPRESSURE         = 1 <<  6;
        const CALLBACK_VTABLE             = 1 <<  7;
    }
}

impl Bindings {
    pub fn caps(&self) -> Capabilities { self.caps }
}

// Backend usage:
impl SwiftBackend {
    fn render_module_imports(&self, b: &Bindings) -> Vec<String> {
        let mut imports = vec!["import Foundation".to_string()];
        if b.caps().contains(Capabilities::ASYNC_NATIVE_AWAIT) {
            imports.push("import _Concurrency".into());
        }
        if b.caps().contains(Capabilities::CALLBACK_VTABLE) {
            imports.push("import boltffi_callback_runtime".into());
        }
        imports
    }
}
```

`Bindings::validate()` enforces that every flag set is used by at least one decl, and that every decl that needs a flag has it set. Capability drift between resolver and backend is a validation failure, not a silent runtime mismatch.

**`FileGroup` trait per backend.** One backend wants three files, another wants one, another wants a header plus an implementation, another wants one file per top-level declaration. The driver does not know any of this. It asks the backend.

```rust
pub trait FileGroup {
    type FileId: Eq + Hash + Clone;
    fn file_for_decl(&self, d: &Decl)                       -> Self::FileId;
    fn imports_for_file(&self, id: &Self::FileId, ctx: &EmitCtx) -> Vec<String>;
    fn header_for_file(&self,  id: &Self::FileId, ctx: &EmitCtx) -> String;
    fn footer_for_file(&self,  id: &Self::FileId, ctx: &EmitCtx) -> String;
}

#[derive(Eq, PartialEq, Hash, Clone)]
pub enum SwiftFile { ValueTypes, ReferenceTypes, Extensions, FFIBridge }

pub struct SwiftFileGroup;
impl FileGroup for SwiftFileGroup {
    type FileId = SwiftFile;
    fn file_for_decl(&self, d: &Decl) -> SwiftFile {
        match d {
            Decl::Record(_) | Decl::Enum(_) | Decl::Constant(_) => SwiftFile::ValueTypes,
            Decl::Class(_)                                       => SwiftFile::ReferenceTypes,
            Decl::Function(_) | Decl::Callback(_) | Decl::Stream(_) => SwiftFile::FFIBridge,
        }
    }
    fn imports_for_file(&self, id: &SwiftFile, _ctx: &EmitCtx) -> Vec<String> {
        match id {
            SwiftFile::FFIBridge => vec!["import Foundation".into(), "@_implementationOnly import boltffi_core".into()],
            _                    => vec!["import Foundation".into()],
        }
    }
    // ...
}
```

The driver collects each backend's `Emitted` chunks (returned by `render_*`), asks the backend's `FileGroup` which file each chunk belongs to, sorts them, prepends imports and headers, and writes the result. Per-language file conventions live here, not in `drive`.

#### Target awareness

`FfiContract` and `AbiContract` are values built from Rust source emitted by the `boltffi_macros` proc macros. Target-specific selection happens before `binding::resolve` runs, by one of two equivalent mechanisms: the proc macros emit `cfg`-gated contract items so the compiler picks the wasm or native shape at the source level, or the user crate selects a target-specific contract module via Cargo features. By the time `binding::resolve` is called, the input is the contract for one specific target.

`binding::resolve` itself does not branch on target. There is no `TargetCapabilities` filter layer. `resolve(ffi, abi) -> Bindings` is target-blind. Backends are target-blind. Target awareness lives where it belongs (in the `cfg` selection that runs before the contract reaches `binding/`), and never crosses the `binding/` boundary.

#### Why this works

Mapped against the five criteria from Section 2:

1. **Decision made once.** The resolver's `if abi_record.is_blittable { Direct } else { Encoded }` is the only place the question is asked. Backends receive the answer as the *type* of the value they handle.
2. **Adding an IR concept breaks the build.** `#[non_exhaustive]` plus exhaustive match in `drive` plus required methods on `Backend`. The compiler points at every site that needs work.
3. **Same trait surface for every backend.** One `supports()` method plus seven `render_*` methods, one per `Decl` variant. Each `render_*` takes a sealed enum that fans out structurally. The driver gates on `supports()` before any `render_*` runs.
4. **Composition is cheap.** Closures plus leaf traits plus shared walkers. Adding a codec leaf is one trait impl. Adding an obligation to a record is one entry in the `sealed_decl!` macro.
5. **New backend cost is bounded.** Implement `Backend`, implement `NameStyle`, implement `FileGroup`, implement the codec and op leaves the language needs. No re-derivation.

#### What stays drifty (by design, and isolated)

The same decision can be spelled differently in different languages. A `DirectRecord` becomes a Swift `struct` with `Equatable`, a Kotlin `data class`, a Java record, a C struct, a TypeScript interface. That spelling is the backend's job and is the entire reason backends exist. Spelling lives in templates and is verified by cross-language conformance tests. The line between "decided once" (semantics, ABI shape, names, symbols, codec, capabilities) and "spelled per language" (templates, idioms, file layout) is the architectural seam.

---

## 4. Decision

We adopt **Alternative 3**. A separate `binding/` crate produces a sealed, validated `Bindings` value from `FfiContract + AbiContract` via `Bindings::from_resolved_parts`, the only public path to a `Bindings`. Backend crates depend on `binding/` only and physically cannot reach into `boltffi_bindgen::ir`. Backends consume `Bindings` through a `Backend` trait whose `render_*` methods correspond one-to-one to `Decl` variants and whose `supports()` advertises the capabilities the backend can render; the driver fails fast when `bindings.caps()` is not a subset of `backend.supports()`. ABI decisions are encoded as the *shape* of the data the backend receives, not as parameters it interprets. Targeted typestate, hidden behind macros and kept closure-local, covers the few silently forgettable obligations without leaking phantom parameters into shared helpers. The supporting pieces (codec tree of `CodecNode`, typed `Op<T>` algebra, snake_case `CanonicalName` plus per-backend `NameStyle`, validated `NativeSymbol` with manifest verification, separate `ReceiverDecl`, `Capabilities` bitflags, `FileGroup` per backend) are part of the same crate and are described in Section 3.3.

Implementation will fill in the leaf set (which primitives, which capability flags, which file groups per language) as backends migrate. Those choices are downstream of this decision and do not change the architecture.

---

## 5. Consequences

### Positive

- The four-answer drift in Section 1.5 collapses. `RecordDecl::Direct` versus `RecordDecl::Encoded` is the answer. `is_blittable_return` and `is_c_style_enum_return` and `is_blittable_decode_seq` cease to exist. There is one fact, encoded in one place, in the type.
- ABI decisions become a single point of change. Today a change to "how records cross" is touched in nine files. After this change, it is touched in one (the resolver) and propagates by compile error.
- Adding an IR concept becomes additive. `cargo build` is the checklist of backends to update.
- New backend cost drops by roughly an order of magnitude. The new backend implements `supports()` plus seven `render_*` methods, a `NameStyle`, a `FileGroup`, and the codec and op leaves it needs.
- Cross-backend conformance tests become meaningful. Every backend operates on the same `Bindings` and the same decisions.
- The class of bug "Swift and Kotlin disagree about the wire shape of a record" becomes structurally impossible. The class of bug "I added a new declaration kind and forgot Python" becomes a compile error rather than a CI miss.

### Negative and costs

- The current backends migrate incrementally. New and old code paths coexist during migration. We accept this transitional cost.
- Some flexibility backends currently have (re-deciding things locally) is removed. In every case we are aware of, that flexibility is the source of the drift we are removing. Where genuine per-language deviation is needed, it lives in the template layer.
- The macros (`sealed_decl!`, `bind_record!`, `bind_function!`) are infrastructure code that must be maintained. The cost is justified because they are the surface that lets the rest of the architecture stay simple.

### Out of scope for this ADR

The items below are deliberately not pinned by this ADR. Each one is downstream of the architecture and would require an ADR amendment on every change if it were included here. The reader should expect them to evolve under normal implementation work without this document needing to be revised.

- **The migration order of backends.** Which backend is ported first, and on what schedule, is a logistics decision. It does not change the architecture, the trait surface, or the data shapes. Pinning a sequence in the ADR would tie scheduling to architectural sign-off.
- **The full list of capability flags.** The architecture commits to "advertise capabilities via bitflags on `Bindings`", and to the validation rule that every flag set is used by at least one decl. The specific set (which async strategies, which error transports, which handle kinds) grows as new runtime features land. Adding `ASYNC_PROMISE_LIKE` later is one bitflag entry plus one validation check; it is not a new architectural decision.
- **The full list of codec leaves.** The architecture commits to "leaves plus shared composers", with backends implementing only the leaves. Which leaves exist (Bool, U8 through I64, F32, F64, String, Bytes, Handle, possibly Decimal128, Uuid, Timestamp) is implementation work that grows additively. Pinning the set in the ADR would require revisiting this document every time a new primitive type is added.
- **Per-language template revisions.** How a `DirectRecord` is spelled in Swift versus Kotlin versus Java is the backend's job and is the reason backends exist. Templates change with idiom, taste, and language version. The architectural seam (Section 3.3, "What stays drifty") is exactly between "decided once" (covered by this ADR) and "spelled per language" (template work).

These are tracked in implementation notes and PRs. They are downstream of accepting this architecture and do not change its shape.

---

## Addendum (2026-08): The fifth answer — macro expansion

Section 1.5 counts four answers to "is this record blittable?", all on the generator side, and the architecture above makes those four structurally one: after the resolver's single branch, a backend cannot disagree because there is nothing to disagree with.

There was a fifth answer the ADR did not cover. The pre-promotion `#[data]` expansion in `boltffi_macros` classified the record again — at proc-macro time, inside the user's crate, where it fixes the compiled library's ABI. That answer came from `ffi_rules::classification`, which was width-blind: it accepted any repr(C) struct of fixed-width primitives, without checking that the field offsets agree across native alignment profiles. The result was the drift of Section 1.5 reproduced across the FFI boundary itself: a mixed-alignment record (`{ i64, i32, f64 }`) compiled into the dylib as raw struct bytes while every IR-backed backend generated wire-encoded access.

Promoting Binding IR generation removed that classifier structurally: the expansion now consumes lowered binding declarations and never re-decides an FFI fact. What remains is making the one rule that survives — `records::is_direct` in `boltffi_binding::lower` — actually sound at the boundary:

- The rule is stated once as `direct_record_fields` and applied by `is_direct`. It requires effective `repr(C)`, at least one field, every field an admissible fixed-width primitive, and byte layout agreement across the supported native ABI alignment profiles.
- Effective `repr(C)` is strict: `repr(C, packed)` and `repr(C, align(N))` change the compiled layout the direct crossing assumes, so such records cross encoded rather than direct with wrong offsets. `repr(packed)` on a `#[data]` record is rejected outright at expansion with a targeted error, because the wire encoder takes field references, which is an error (E0793) on packed structs.

The same "decided once, then structural" property the IR gives backends is the end state for the macro side as well: the per-invocation wrapper work (RFC #665) can make the expansion's crossing representation a *type-level* consequence of this classification, at which point disagreement between the compiled ABI and the generated bindings becomes inexpressible rather than tested-for.
