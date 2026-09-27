# BoltFFI documentation

The website is built with Astro and MDX. The same pages are published as HTML, Markdown, and the combined `llms-full.txt` document.

## Run locally

From this directory, with Node.js 24:

```sh
npm ci
npm run dev
```

Open `http://localhost:4321/docs/overview`. To build the site and verify its Markdown output:

```sh
npm run build
```

## Edit a page

Pages live in `src/content/docs`. `CodeComparisonWrapper.astro` displays Rust beside the generated-language examples. It supports Swift, Kotlin, Java, C#, TypeScript, Python, and C. `TypeTableWrapper.astro` displays the corresponding type mappings.

Keep code examples consistent with generated bindings. C examples must show cleanup for returned owners and distinguish input views from owning values. Do not add a C example for an API the target cannot generate; explain the limitation on the relevant topic page. Keep C type mappings and API examples beside the other languages. The C page covers linking and memory management; build commands belong in Packaging and settings belong in Configuration.

When adding a page, register it in `src/lib/documentation/index.ts` and the sidebar components. The build checks that every page appears in both Markdown indexes. The Markdown renderer in `src/lib/documentation/markdown.ts` must preserve every supported language when the comparison components change.

The [C demo](../examples/platforms/c) builds and runs against the generated header on the supported CI hosts.
