# ADR: Generated Name Uniqueness. One Check for Every Backend

## Status
Proposed

## Authors
- [Henrik](https://github.com/akesson)

## Date
2026-10-29

---

## 1. Background and Problem

### 1.1 How names collide

Backends derive target names by joining Rust names. Joining loses word boundaries:

```rust
#[export]
impl Engine {
    #[ffi_stream(item = i32)]
    pub fn values(&self) -> Arc<EventSubscription<i32>> { … }
    pub fn values_subscribe(&self) -> u64 { … }
}
```

```csharp
// stream subscribe wrapper:   Native + Engine + Values + Subscribe
internal static extern ulong NativeEngineValuesSubscribe(ulong receiver);
// method wrapper:             Native + Engine + ValuesSubscribe
internal static extern ulong NativeEngineValuesSubscribe(ulong receiver);
```

Generation succeeds and `dotnet build` fails with CS0111. The error names
`NativeMethods`, not the two Rust declarations. Issue #700 lists the cases that
fail on main today.

### 1.2 What exists today

Each backend has added checks for the cases its authors hit:

| backend | check |
|---|---|
| Python | `NameScope`, `PythonNameCollision` |
| Java | `JavaNameCollision` at eight call sites, including `validate_signatures` |
| Kotlin | `KotlinNameCollision` |
| C | `CNameCollision` |
| C# | `CSharpModuleClassCollision`, module class only (#956) |
| Swift | none |

The checks differ in what they cover. `get_url` + `getUrl` is rejected by Java
and Python and reaches the compiler in C#, Kotlin and Swift. Every new derived
name, such as a new companion class, needs someone to remember to extend the
check for that backend.

Python's `NameScope` already has the right shape. It inserts each name with the
declaration that produced it and fails with both on a duplicate. This ADR moves
that idea into core.

## 2. Decision

Renderers record every public name they emit as a claim. The driver checks all
claims from all declarations before `assemble` runs. A duplicate fails
generation and the error names both Rust declarations.

### 2.1 Claims

```rust
pub struct Claim {
    scope: ScopeId,             // where the name must be unique
    name: String,
    signature: Option<String>,  // the overload key, for overloadable members
    kind: ClaimKind,            // Declare, Shared or Reserve
    role: String,               // "P/Invoke wrapper for `values_subscribe`"
}
```

A backend defines its own scopes. For C# those are the namespace, each type's
member list, `NativeMethods`, and the generated files. The files scope compares
names case-folded, because `Demo.cs` and `demo.cs` are one file on macOS and
Windows.

A member claim carries an overload key, because overloads are legal.
`NativeFoo(ulong)` and `NativeFoo(string)` do not collide. The backend computes
the key from its own overload rules, so it is not always the rendered parameter
list. Java erases generics, so `f(List<String>)` and `f(List<Integer>)` get the
same key, as they do in `ErasedSignature` today. A claim without a key, such as
a property or a nested type, collides with every member of the same name.

`ClaimKind` covers the three ways a name is used:

- `Declare` emits the name. Two `Declare` claims on one key collide.
- `Shared` is support code that many declarations emit identically, such as
  `WireReader`. It carries a stable helper identity like `HelperId`. `Shared`
  claims with the same identity do not collide. Different identities do.
- `Reserve` keeps a name free without emitting it, for example a keyword,
  `NativeMethods`, or a type's own name inside its member list (CS0542).

### 2.2 Recording and checking

A renderer receives `&RenderContext` and cannot write shared state, so its
claims travel on `Emitted` next to the aux chunks:

```rust
Emitted::primary(source).with_claim(
    Claim::declare(&native_methods, &self.native_name, "P/Invoke wrapper for `values_subscribe`")
        .with_signature("(ulong)"),
)
```

Names the host emits outside any one declaration, such as the module class and
`NativeMethods`, come from a new `HostBackend::host_claims` hook. Its default is
empty, so a backend that has not migrated behaves as before.

Claims cover names that hosts render. Bridges return `GeneratedOutput` from
`render_bridge` and make no claims. C symbols are already unique because the
binding IR length-prefixes their paths, and `Java_*` symbols derive from the JVM
class and method names. A bridge that derives other names needs its own claim
path before this check covers them.

`Target::render_with_coverage` already renders every declaration before it calls
`host.assemble`. The check goes between the two. The driver labels each claim
with the declaration that produced it, using the Rust spelling. A claim from
`host_claims` has no Rust declaration, so the driver labels it with the host
setting or built-in name instead. The driver collects all collisions, so one run
reports all of them, and no file is written if there are any.

In partial mode, the C host drops declarations inside `assemble`, for example
when a record has no C codec. A dropped declaration must not cause a collision,
so that filtering moves to a new hook that runs before the check. Its default
keeps every declaration.

```
generated name collision in the csharp backend
  `NativeEngineValuesSubscribe(ulong)` in `NativeMethods` is derived by both:
    - class `Engine` (P/Invoke wrapper for `values_subscribe`)
    - stream `Engine::values` (stream subscribe wrapper for `values`)
rename one of the Rust declarations so the generated names differ
```

When a collision involves a host name, the last line names the setting to change
instead. A C# `module_class` of `NativeMethods` collides with the built-in class,
and the fix is in the configuration, not the Rust code.

### 2.3 Rejected or renamed

Public names that users call are rejected on collision. The user renames the
Rust declaration. Generating `GetUrl2` instead would make the public API depend
on declaration order.

Private names inside a function body may still be renamed. #940 does this for C#
helper locals that clash with parameter names such as `status`, and
`LexicalPlan` does it for local variables. Those names are not claimed.

## 3. Rollout

1. Core types, claims on `Emitted`, the `host_claims` hook, and the check in
   the driver, together with the C# migration. C# fails in six of the seven
   cases in #700, and #956's checks fold into claims.
2. Java, Kotlin, Python and C. Their `*NameCollision` errors become claims, and
   the C host moves its partial-mode filtering to the new hook.
3. Swift, then the remaining backends.

Each step is one PR with regression tests that compile the generated code.

## 4. Alternatives considered

### 4.1 Keep checks per backend

This is the current approach. Coverage differs between backends, and each new
derived name needs its own check in each backend.

### 4.2 Rename on collision

Appending a suffix keeps generation working. It also changes public names when
someone adds or reorders a declaration, which breaks callers in the target
language.

### 4.3 Check only at the file writer

#992, still open, proposes rejecting duplicate output paths in
`Generation::write_output`. It only sees paths that reach the writer, and
`GeneratedOutput::append` has already merged exact duplicates by then. It cannot
see two types merged into one file, a member clash inside a file, or which Rust
declarations caused the clash. This ADR does not depend on it, because the files
scope covers the paths hosts generate.

### 4.4 Enforce claiming in the type system now

Template fields could take a `ClaimedIdentifier` that only claiming can
construct, so a renderer that skips claiming would not compile. That changes
field types in every renderer. It can follow once the claims have run in a few
backends.

## 5. Consequences

- A collision fails at generation with both Rust declarations named, in every
  backend that has migrated.
- New derived names are covered as soon as the renderer claims them, without a
  separate check.
- Renderers do a little more work per name. The table is one hash map per run.
- Until a backend migrates, its collisions behave as they do today.
