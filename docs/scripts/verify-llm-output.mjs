import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';

const sourceDirectory = new URL('../src/content/docs/', import.meta.url);
const outputDirectory = new URL('../dist/docs/', import.meta.url);
const sourceFiles = await readdir(sourceDirectory);
const pageIds = sourceFiles
  .filter((file) => file.endsWith('.mdx'))
  .map((file) => file.slice(0, -4))
  .sort();
const pages = await Promise.all(
  pageIds.map(async (id) => ({
    id,
    markdown: await readFile(new URL(`${id}.md`, outputDirectory), 'utf8'),
    html: await readFile(new URL(`${id}/index.html`, outputDirectory), 'utf8'),
  })),
);
const llmsIndex = await readFile(
  new URL('../dist/llms.txt', import.meta.url),
  'utf8',
);
const llmsFull = await readFile(
  new URL('../dist/llms-full.txt', import.meta.url),
  'utf8',
);

pages.forEach(({ id, markdown, html }) => {
  assert.match(markdown, /^# /, `${id}.md must start with a title`);
  assert.doesNotMatch(
    markdown,
    /(?:CodeComparison|TypeTable|Wrapper\.astro)/,
    `${id}.md contains MDX implementation details`,
  );
  assert.match(
    html,
    new RegExp(`rel="alternate" type="text/markdown" href="https://boltffi\\.dev/docs/${id}\\.md"`),
    `${id} HTML does not advertise its Markdown representation`,
  );
  assert.match(
    html,
    /Copy for LLM/,
    `${id} HTML does not render the LLM copy action`,
  );
  assert.match(
    llmsIndex,
    new RegExp(`https://boltffi\\.dev/docs/${id}\\.md`),
    `${id}.md is missing from llms.txt`,
  );
  assert.match(
    llmsFull,
    new RegExp(`Source: https://boltffi\\.dev/docs/${id}(?:\\n|$)`),
    `${id} is missing from llms-full.txt`,
  );
});

assert.match(
  pages.find(({ id }) => id === 'constants')?.markdown ?? '',
  /Constants can use the same types as exported function results/,
);

const cExamplePages = ['overview', 'types', 'records', 'classes', 'functions', 'errors', 'constants'];
cExamplePages.forEach((id) => {
  const page = pages.find((candidate) => candidate.id === id);
  assert.match(page?.markdown ?? '', /```c\n/, `${id}.md lost its C examples`);
  assert.match(page?.html ?? '', />C<\/button>/, `${id} has no C language button`);
});

const typeMarkdown = pages.find(({ id }) => id === 'types')?.markdown ?? '';
assert.match(typeMarkdown, /\| C\s*\|/, 'the Markdown type tables lost their C column');
assert.match(typeMarkdown, /DemoStringView \/ DemoString/, 'C ownership types are missing');
const cMarkdown = pages.find(({ id }) => id === 'c')?.markdown ?? '';
assert.match(cMarkdown, /demo_string_free/, 'the C page lost its cleanup example');
assert.match(cMarkdown, /## Memory management/, 'the C page lost its ownership rules');
const errorMarkdown = pages.find(({ id }) => id === 'errors')?.markdown ?? '';
assert.match(errorMarkdown, /demo_parse_int_result_free/, 'the Errors page lost C result cleanup');
const callbackMarkdown = pages.find(({ id }) => id === 'callbacks')?.markdown ?? '';
assert.match(callbackMarkdown, /demo_value_callback_create/, 'the Callbacks page lost the C vtable example');

console.log(`verified ${pages.length} LLM-ready documentation pages`);
