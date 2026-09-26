# Contributing to BoltFFI

Anyone is welcome to help with BoltFFI. If you are not sure where to start, open an issue, ask on Discord, or send a draft PR early so we can help you find the right path before the change gets too large.

- Discord: [Join the community](https://discord.gg/Q6A7zNNFk3)
- Issues: [boltffi/boltffi issues](https://github.com/boltffi/boltffi/issues)

By contributing, you agree that your work is licensed under the project license, [MIT](../../LICENSE).

## AI-assisted contributions

AI tools are welcome when they help a human contributor do the work. The contributor must understand, verify, and take responsibility for the result. Read the [AI policy](../../AI_POLICY.md) before submitting an issue, commit, pull request, or review comment.

## Building the project

You need a working Rust toolchain. We recommend installing Rust through [`rustup`](https://rustup.rs/) and using the toolchain pinned by the repository.

From the repository root, run:

```bash
just build-all
```

That should build the Rust crates. If it does not, please file an issue with the command you ran, the platform, and the error.

## Running the tests

For the full Rust test suite:

```bash
just test
```

Before sending a PR, also run:

```bash
just fmt-check
just lint
```

BoltFFI generates bindings for several languages, so not every platform test can run on every machine. For fast iteration, run the focused crate test first, then the demo path that matches the work.

For example, backend changes usually start with:

```bash
just test-crate boltffi_backend
```

Python target changes can be checked through the demo:

```bash
just demo-verify --platform python --python python
```

The demo audit checks that every promised demo behavior is either covered by a platform test or explicitly excluded with a reason:

```bash
just demo-test-audit
```

If a target is still being brought up, do not fake coverage. Add an exclusion with a real reason, implement the feature, then remove the exclusion when the platform test exists.

## Navigating the code

If you are new to BoltFFI, start with `examples/demo`. It is the best map of what the project promises to support. Each folder in `examples/demo/src` covers one feature area, and the platform tests show what the generated bindings are supposed to feel like from another language.

The main crates are:

- `boltffi`: the public runtime facade used by crates that expose BoltFFI APIs.
- `boltffi_core`: runtime pieces shared by generated code.
- `boltffi_macros`: procedural macros and Rust wrapper expansion.
- `boltffi_ast`: the source contract model.
- `boltffi_scan`: parser from annotated Rust items into source fragments.
- `boltffi_binding`: the Binding IR and lowering rules. This is the source of truth for boundary decisions.
- `boltffi_backend`: IR-based bridge and target renderers.
- `boltffi_bindgen`: metadata extraction and generation driver for the new backend path.
- `boltffi_cli`: the `boltffi` command, packaging, and user-facing configuration.
- `boltffi_tests`: integration tests that tie scanner, binding, and generated behavior together.

There is still old rendering code in the repository. Do not copy it into new work. The new path is `fragments -> ast -> bindings -> backend`: each macro invocation describes its own item, and bindgen reads those descriptions from the compiled artifact. New targets use `boltffi_backend`, not `boltffi_bindgen/src/render/<lang>/`.

The macros and bindgen learn about a crate without reading its source files. [Per-invocation expansion](./per-invocation-expansion.md) walks one crate through every expansion, and [ADR 0002](../../adrs/0002-adr-declaration-lanes.md) explains why it works this way.

## Working on the backend path

If you are adding or changing a target language, start with [Adding a new target language](./adding-a-target.md). It explains the backend layout, bridge stack, capabilities, demo loop, and when a feature is allowed to become stable.

## Finding something to work on

Good first patches are usually small and visible:

- Fix a failing demo case for one target.
- Improve an error message that points at the wrong thing.
- Add a missing platform test for behavior the target already supports.
- Fill a gap in the scanner or Binding IR with a focused test.
- Improve contributor docs where the current path is confusing.

Adding a new target is welcome, but it should be incremental. Wire the target, gate it as experimental, get one feature working end to end, and then grow it feature by feature through the demo.

If you want to make a bigger architectural change, open an issue first. BoltFFI has a few layers that intentionally prevent drift between targets. Moving a decision to the wrong layer can work locally and still make the system worse.

## Sending a pull request

Changes should be submitted as pull requests.

Before sending a PR:

- Base the branch on current `main`.
- Keep the PR focused. One feature, one fix, or one mechanical cleanup is easier to review than a mixed rewrite.
- Add tests for the behavior you changed, or explain why the change does not need a test.
- Run the focused `just test-crate <crate>` check for the crate you touched.
- Run `just fmt-check`.
- Run `just lint`.
- For generated code changes, include a test or fixture that shows the generated output changed on purpose.
- For demo-facing behavior, update the demo marker or exclusion so `just demo-test-audit` stays honest.

Draft PRs are welcome. Use one when the shape is still being discussed or when the target is still moving through the demo feature list.

Please do not include merge commits in pull requests. Keep commits about the change itself.

## Code review

Every pull request needs review before it can merge. Reviewers may ask for tests, smaller commits, clearer naming, or a different implementation shape.

If a review comment is unclear, ask for clarification. It is better to slow down and agree on the direction than to keep changing the patch blindly.

## Merging code

A pull request can merge when the relevant tests pass and the change has been approved. If CI is still running after review, the author or maintainer can enable auto-merge once the branch is ready.

For target bring-up work, do not call a capability stable until the demo cases for that declaration kind pass for the target with no skipped declarations and no exclusions for that kind. Partial coverage is a work queue, not a stability signal.
