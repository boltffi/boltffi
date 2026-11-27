# ADR: Wire Format. One Specification, Checked by Shared Vectors

## Status
Proposed

## Authors
- [Henrik](https://github.com/akesson)

## Date
2026-11-27

---

## 1. Background and Problem

### 1.1 One value, different bytes per target

```rust
#[data]
pub struct Page { pub offset: usize, pub title: String }

#[export]
pub fn make_page() -> Page { Page { offset: 7, title: "abc".into() } }

#[export]
pub fn page_title_len(page: Page) -> u32 { page.title.len() as u32 }
```

The Rust runtime encodes the returned `Page` as 15 bytes:

```
07 00 00 00 00 00 00 00   03 00 00 00   61 62 63
offset, 8 bytes           title length  "abc"
```

The TypeScript runtime reads and writes `usize` as 4 bytes. On main at
`2f93f706`, built for wasm and run in Node:

```
makePage()                                -> { offset: 7, title: "" }
pageTitleLen({ offset: 7, title: "abc" }) -> 0
```

Neither call reports an error. In the first, TypeScript reads the title length
from the upper half of `offset`. In the second, Rust rejects the bytes and the
call returns 0.

The binding IR already states the rule: `Primitive::wire_size` documents that
"platform-width integers always occupy eight bytes on the wire". C#, Python,
Swift and Kotlin write eight. Nothing checks the rule.

### 1.2 The format is defined only by code, and the code has disagreed

The wire format has no document. It is the Rust runtime in
`boltffi_core/src/wire`, the binding IR (`CodecNode`, `Primitive::wire_size`,
the record layout), and one hand-written reader and writer per target. A new
backend learns it by reading one of the existing runtimes.

| | what disagreed | found by |
|---|---|---|
| #202, open | TypeScript wrote a `Vec` of direct records packed, Rust read the `repr(C)` image | a parity test |
| #203, open | TypeScript wrote a nested `Vec<Vec<isize>>` wrong | a parity test |
| #204, open | width of `isize` and `usize` elements in a `Vec` | reading the code |
| #195, open | width of a C-style enum on the wire | a review comment |
| #780, open | Kotlin read a padded direct record back to back | a user |
| #1043, merged | Dart, C# and Swift read and wrote a padded direct record back to back | a contributor |
| 1.1 above | TypeScript reads and writes `isize` and `usize` as 4 bytes | writing this ADR |

Each was found on its own. Four of the open ones look fixed on main (rollout
step 2), and nothing would show it if they broke again.

Two rows work in one position and fail in another. #1043 was a direct record
inside an `Option`, while the same record in a `Vec` worked. In 1.1 a `usize`
fails as a record field, while `Vec<Vec<isize>>` echoes correctly on the same
wasm build.

The demo's 440 `demo_case`s did not catch these, because their shapes are
picked by hand. No demo record has an `isize` or `usize` field.

### 1.3 What nothing decides

- **Pointer-width integers in a sequence** (#204). Bytes per `isize`, from the
  generated code:

  | | scalar | in a `Vec` inside a buffer | root `Vec`, a direct vector |
  |---|---|---|---|
  | Rust | 8 | pointer width | pointer width |
  | Swift, C# | 8 | 8 | pointer width |
  | Kotlin, Java | 8 | 8 | 8 |
  | TypeScript, on wasm32 | 4 | 4 | 4 |

  On a 64-bit target the columns agree. On a 32-bit target Rust disagrees with
  every host but TypeScript inside a buffer, and with the JVM hosts in a direct
  vector. Only wasm has been run.
- **Invalid bytes.** An `Option` tag of 2 is an error in Rust, an exception in
  Kotlin, a `fatalError` in Swift and `Some` in TypeScript and Java. A scalar
  `bool` byte of 2 is an error in Rust and `true` in Swift, Kotlin, Java, C#,
  Dart and TypeScript.
- **Who may keep the bytes.** #666 asks for the generated record codecs to be
  public. Nothing says whether one build may read bytes another build wrote.

Enum tag widths (#195), padding bytes and map order are decided, but only in
code.

## 2. Decision

The wire format gets one specification. One set of vectors checks the Rust
runtime and every target against it in CI. The set covers every shape in every
position and is not picked by hand.

### 2.1 The specification

`docs/contributors/wire-format.md` is the normative text. It covers the bytes of
an encoded value. It does not cover how a call passes scalars, direct vectors,
handles and errors, or the async and stream protocols.

The table is what the Rust runtime does today, except for 2.2 and 2.4.

| value | bytes |
|---|---|
| integers, floats | fixed width, little-endian |
| `isize`, `usize` | 8 bytes, as `i64` and `u64` (2.2) |
| `bool` | 1 byte, `0` or `1` |
| `String`, `&str` | `u32` byte length, then UTF-8 |
| `Vec<u8>` | `u32` length, then the bytes |
| `Option<T>` | 1 byte: `0`, or `1` then `T` |
| `Result<T, E>` | 1 byte: `0` then `T`, or `1` then `E` |
| `Vec<T>` | `u32` count, then each element |
| map | `u32` count, then each key and its value (2.3) |
| tuple | each element in order |
| encoded record | each field in declaration order |
| direct record | its `repr(C)` image: fields at their offsets, size rounded up to the alignment (2.3) |
| C-style enum | its `repr` integer, `i32` when none is declared |
| data enum | `i32` variant index, then the variant's fields |
| `Duration` | `u64` seconds, `u32` nanoseconds below 10^9 |
| `SystemTime` | `i64` seconds since the epoch, `u32` nanoseconds below 10^9 |
| `Uuid` | two `u64`, the high half first |
| `Url` | as a `String` |
| custom type | as its representation |
| interned string | 1 byte: `0` then a `u32` pool id, or `1` then a `String` |

Encoded values follow each other with no padding and no alignment:

```
Some(Level::High)       #[repr(u8)], High = 200     01 c8
Shape::Label("hi")      second variant              01 00 00 00  02 00 00 00  68 69
Ok::<i32, String>(42)                               00 2a 00 00 00
Some(Padded { tag: 1, value: 0x01020304, flag: 7 }) 01  01 00 __ __  04 03 02 01  07 __ __ __
```

`Padded` is `{ tag: u16, value: u32, flag: u8 }`, and `__` is padding.

A buffer holds one value and nothing after it. A class or a callback is not a
wire value. It crosses on its own as a handle.

The root of a call can differ per surface. wasm passes a root `String` and a
returned `Vec<u8>` without the length prefix, since the call carries the
length, and an `Option` of a small scalar in a scalar slot. The specification
lists these root forms per surface.

### 2.2 Pointer-width integers are 8 bytes in an encoded buffer

This is the rule `Primitive::wire_size` documents, applied to sequence elements
too: a `Vec<isize>` inside a buffer is a count and 8 bytes per element. Swift,
Kotlin, Java and C# already read and write that. Rust copies a sequence as
memory only when the element's memory image is its wire form, so on a 32-bit
target it converts `isize` and `usize` elements. TypeScript follows.

A direct vector is not an encoded buffer. It is the elements' memory, passed as
a pointer and a length, and its elements have their C width: the generated
header declares `const intptr_t *`. Rust, Swift, C#, Python, Dart and
TypeScript already agree. The JNI bridge passes a `jlong` array as that
pointer, which is wrong on 32-bit Android, and converts there.

This settles #204 with the least change: a direct vector keeps its memory copy
and its borrow in place on every target.

### 2.3 Padding and map order are not part of the value

A writer may leave any bytes in the padding of a direct record, and a reader
ignores them. Map entries may come in any order. The vectors treat encodings
that differ only in these as equal.

### 2.4 Invalid bytes

Both ends are generated from one contract and run in one process, so the wire
is not a trust boundary. A decoder still checks what is cheap, so that a codec
disagreement is reported and does not become a wrong value, a read outside the
buffer or undefined behavior.

| bytes | a decoder | today, where checked |
|---|---|---|
| a length or count past the buffer | rejects, before allocating from it | Swift's string reader does not check. Rust, Swift, Kotlin, Java, C# and TypeScript allocate from a count first |
| an `Option`, `Result` or enum tag that names no case | rejects | TypeScript and Java read an `Option` tag of 2 as `Some` |
| a scalar `bool` other than `0` or `1` | rejects | Swift, Kotlin, Java, C#, Dart and TypeScript read `true` |
| an `isize` or `usize` outside the reader's pointer width | rejects | Rust truncates on a 32-bit target |
| bytes after the value | rejects | Rust ignores them. No host sends any: the Swift, Kotlin, Java, TypeScript, Python, Dart and C demo suites pass with the check in Rust |
| a string that is not UTF-8 | trusts the writer, and Rust rejects in a debug build | Rust trusts the strings an export receives, at the root and nested |
| nanoseconds of 10^9 or more, a duplicate map key | may reject | Rust rejects both |

A `bool` inside a memory copy, in a direct record or a `Vec<bool>`, is trusted.

UTF-8 is a precondition on the writer, stated in the specification and in the
C header. Every host's encoder writes UTF-8, so only a hand-written C caller
can break it, with undefined behavior in a release build, as with a bad
pointer. Checking it in Rust would cost about 2 ns for a short ASCII string and
430 ns for 1 KB of non-ASCII text.

A rejection is an error, or a trap with a message where the call has no error
path, as in Swift. It is never a different value.

How Rust reports a rejected argument is a call-protocol question, and this ADR
leaves it as it is. Rust records a last-error message, which only the C target
reads, and returns an empty value: the 0 in 1.1. The vectors catch such a
disagreement in CI. A production call still returns a wrong value until that
is decided.

### 2.5 The bytes belong to one build

Bytes are valid only between a library and bindings generated from the same
contract. They are not a storage or network format, carry no version, and may
change in any release. This does not decide #666, but it is the promise public
codecs would come with.

### 2.6 Vectors

A vector is a type, a value and its bytes. The vectors are defined once, in a
`wire` module of the demo crate, and used two ways.

**Rust pins the bytes.** A Rust test encodes every vector and compares the
result with a committed file. A PR that changes that file changes the format:
it updates the specification with it and is reviewed as an ABI change.

**Each target echoes them.** For each vector type the module exports a count
and two functions:

```rust
#[export]
pub fn page_vector(index: u32) -> Page { /* the index-th value */ }

#[export]
pub fn page_check(index: u32, value: Page) -> bool { /* equals the index-th value */ }
```

A target's test is the same loop for every type and holds no expected values:

```kotlin
for (index in 0u until pageVectorCount()) check(pageCheck(index, pageVector(index)))
```

The target decodes Rust's bytes and encodes them back, and Rust compares what
it decodes. A codec with the wrong width, offset or order fails, which covers
every row in 1.2. `check` returns a `bool` because a rejected argument returns
the empty value (2.4), and that must read as a failure.

**A vector type is a position.** Each type holds a group of leaves in one
position:

```rust
#[data]
pub struct ScalarsInOption { pub i8: Option<i8>, /* the group */ pub text: Option<String> }
```

- leaf groups: the primitives including `isize` and `usize`, with strings; a
  direct record with padding, an encoded record, a C-style enum with a
  `repr(u8)` value above 127, a data enum, each builtin and a custom type; and
  bytes with a `Vec` of each primitive and of the direct record, which Rust
  copies as memory,
- positions: a record field, an enum payload, and inside `Option`, `Vec`,
  `Result`, a tuple and a map. A leaf that crosses as a buffer or a direct
  vector is also a vector type of its own at the root.

Each group is written once and a macro stamps it into each position, so a new
leaf lands in every position. That is about 45 types. A failing type names the
group and the position, and the leaf is found by hand.

The groups limit what one gap costs. Java cannot render a record field that
holds a map of arrays, so it excludes the third group in the map position and
nothing else.

A spike stamped 25 leaves into four positions. It generates on Swift, Kotlin,
TypeScript, Python, C#, Dart and C. On the wasm build from 1.1 the field,
`Option` and map types fail on their pointer-width leaves and the `Vec` type
passes, as the table in 1.3 predicts.

Echoed values are ones every target's public type holds exactly. Swift keeps a
`SystemTime` as a `Double` and TypeScript as milliseconds, so echoed times are
whole and half seconds. Values a host cannot hold, such as a nanosecond time or
`usize::MAX`, are pinned in Rust and not echoed.

Each type is one `demo_case` on its exports, which are written by hand because
the audit reads cases from the source text. Its marker goes in the target's
test, so the existing audit makes every target pass the type or exclude it. A
target may exclude a type only when it cannot render a shape in it. A target
that renders the shapes and gets the bytes wrong has a bug. ADR 0005 (#1062)
makes the audit check such an exclusion against the target's capability table.
Until it lands, the exclusion names an issue.

### 2.7 What CI checks

| check | job | new |
|---|---|---|
| Rust's bytes equal the pinned file | `demo-test-audit` | the test, and a `cargo test` step for the demo crate, which no job runs today |
| the same on a 32-bit target, from rollout step 3 | `demo-test-audit` | a 32-bit Linux run of the test |
| each target echoes every vector | `demo-platforms`, on macOS, Ubuntu and Windows | the loops |
| no target leaves a vector out without a gap entry | `demo-test-audit` | the audit rule from ADR 0005 |
| the byte examples in the specification are current | `demo-test-audit` | a test that regenerates them from the pinned file |
| every `CodecNode` variant has a vector | `test` and `demo-test-audit` | a `match` in `boltffi_binding` with no wildcard arm names each variant's leaf, or why it has none (a handle, a root form). The pinned-file test fails for a leaf with no vector |
| a runtime rejects the bytes in 2.4 | `test` for Rust, the TypeScript runtime tests, and for the others a fixture compiled into the generated module, as `wire_tags.kt` is for Kotlin | the fixtures, and a toolchain in the job that runs each |

The 32-bit run checks 2.2: Rust's encoded bytes are the same on both widths,
and no host's buffer codec depends on the width. A direct vector on a 32-bit
host runs only on wasm. The JNI bridge on 32-bit Android is not run in CI.

The echo loop does not show that a target presents a value correctly: one that
reads a `u32` as signed echoes the same bytes. The hand-written demo cases keep
covering that. Generated functions cannot send invalid bytes, which is why
rejection is tested with fixtures.

## 3. Rollout

1. The specification, the `wire` module and the pinned file, with the Rust
   checks from 2.7 except the 32-bit run and the rejections. No behavior
   changes: where the runtime differs from 2.2 the file records today's 64-bit
   bytes. Every target starts excluded.
2. One PR per target adds the loop and fixes what fails. Known to fail:
   TypeScript `isize` and `usize` (1.1). #202, #203, #780 and the enum width
   in #195 look fixed on main: the shapes from the first two echo correctly on
   a wasm build, and the generated code for the other two reads the right
   widths and offsets. Each closes when its vectors pass.
3. Pointer-width integers in sequences (2.2): Rust converts on 32-bit targets,
   with the 32-bit run, and the JNI bridge converts direct vectors. This
   closes #204.
4. The rejections in 2.4, one runtime at a time, Rust first. Each starts from
   a survey of what the runtime accepts today and ends with its fixture.
5. `adding-a-target.md` makes the first leaf group a new target's first
   milestone.

## 4. Alternatives considered

### 4.1 Keep adding parity tests per target

This is the current approach. Each test covers the shape its author thought
of. #1043 and 1.1 both failed in a position that had no test.

### 4.2 Write the specification without vectors

The IR documents the 8-byte rule, and the TypeScript runtime changed from 8
bytes to 4 in July (`c9a950ef`) without a failing test.

### 4.3 Byte fixtures that each target decodes without Rust

A file of bytes and expected values, read by a unit test in each language,
would also check how a value is presented. It needs the generated codecs to be
reachable from tests in every target, and a value notation every language
parses. The echo loop needs neither and catches every disagreement found so
far. Fixtures can follow for what the loop cannot see.

### 4.4 One shared codec implementation

A codec written once would remove this class of bug. Each target reads and
writes the wire with its own types because that is where BoltFFI's speed comes
from, and ADR 0001 has each renderer walk the `CodecNode` tree for that reason.

### 4.5 Adopt an existing format

Protobuf and postcard have no record that crosses as its memory image.
FlatBuffers and Cap'n Proto do, but they bring their own layout rules for every
other type, and every runtime and renderer would be rewritten to them. Other
tools reading BoltFFI's bytes is not a goal (2.5).

### 4.6 One width for pointer-width integers everywhere

8 bytes in a direct vector too would give one rule. It changes Rust, the C
header, Swift, C#, Python, Dart and TypeScript, which agree today, and on a
32-bit target a `&[isize]` could no longer be borrowed in place. Only the JVM
bridge would stay as it is.

Pointer width inside a buffer would keep Rust's memory copy on 32-bit targets.
A host's buffer codec would then depend on the architecture, and JVM and .NET
bindings are built once for several.

### 4.7 One vector type per leaf and position

A type for every combination names the failing leaf and lets a target exclude
one combination. It is about 300 types and 900 exports, where the demo crate
has 476 exports today, with a loop per type in each of nine targets. The audit
reads `demo_case` from source text and cannot see macro-generated items, so it
would need a generator for the cases and the loops as well.

One type per position with every leaf is about 30 types, but one gap, such as
Java's above, excludes a whole position.

## 5. Consequences

- A new backend has a document to implement, and vectors to pass from its
  first leaf group.
- A codec disagreement fails CI for every leaf in every position, before a
  user meets that combination.
- A format change is a diff in the pinned file and the specification.
- The demo crate gains about 45 vector types with three exports each, about a
  quarter more exports, and each target's test gains a loop per type.
- A `Vec<isize>` or `Vec<usize>` inside a buffer loses Rust's memory copy on
  32-bit targets, wasm included. Direct vectors do not change, except in the
  JNI bridge on 32-bit Android.
- TypeScript reads and writes pointer-width integers in a buffer as 8 bytes.
  The public type stays `number`.
- Runtimes that accept invalid bytes reject them. Rust checks counts before
  allocating, the range of a pointer-width integer and bytes after the value.
  It checks UTF-8 in debug builds only.
- How Rust reports a rejected argument to the host is still undecided.
- Which shapes a target must support is ADR 0005's subject. Its corpus checks
  that a shape generates and compiles, and these vectors check the bytes.
