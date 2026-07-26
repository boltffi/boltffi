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

console.log(`verified ${pages.length} LLM-ready documentation pages`);
