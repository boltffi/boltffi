import type { CollectionEntry } from 'astro:content';
import { DocumentationMarkdown } from './markdown';

type DocumentationEntry = CollectionEntry<'docs'>;

interface DocumentationSection {
  title: string;
  pages: readonly string[];
}

export interface DocumentationNeighbors {
  previous?: DocumentationPage;
  next?: DocumentationPage;
}

const DOCUMENTATION_SECTIONS: readonly DocumentationSection[] = [
  {
    title: 'Start here',
    pages: ['overview', 'getting-started', 'installation', 'quick-start', 'tutorial'],
  },
  {
    title: 'Design a Rust API',
    pages: [
      'types',
      'records',
      'classes',
      'functions',
      'constants',
      'async',
      'callbacks',
      'streaming',
      'errors',
      'custom-types',
    ],
  },
  {
    title: 'Build and package',
    pages: ['packaging', 'c', 'configuration'],
  },
  {
    title: 'Internals and experimental features',
    pages: ['experimental', 'async-internals'],
  },
];

export class DocumentationPage {
  constructor(readonly entry: DocumentationEntry) {}

  get id(): string {
    return this.entry.id;
  }

  get title(): string {
    return this.entry.data.title;
  }

  get description(): string {
    return this.entry.data.description;
  }

  get htmlPath(): string {
    return `/docs/${this.id}`;
  }

  get markdownPath(): string {
    return `${this.htmlPath}.md`;
  }

  markdown(): string {
    return DocumentationMarkdown.render(this.entry.body ?? '');
  }

  markdownUrl(site: URL): string {
    return new URL(this.markdownPath, site).toString();
  }

  htmlUrl(site: URL): string {
    return new URL(this.htmlPath, site).toString();
  }

  markdownResponse(site: URL): Response {
    return new Response(this.markdown(), {
      headers: {
        'Content-Type': 'text/markdown; charset=utf-8',
        Link: `<${this.htmlUrl(site)}>; rel="canonical"`,
      },
    });
  }
}

export class DocumentationCorpus {
  readonly pages: readonly DocumentationPage[];

  constructor(entries: readonly DocumentationEntry[]) {
    const pagesById = new Map(
      entries.map((entry) => [entry.id, new DocumentationPage(entry)]),
    );
    const orderedIds = DOCUMENTATION_SECTIONS.flatMap(({ pages }) => pages);
    const unknownIds = entries
      .map(({ id }) => id)
      .filter((id) => !orderedIds.includes(id));

    if (unknownIds.length > 0) {
      throw new Error(`Documentation navigation is missing: ${unknownIds.join(', ')}`);
    }

    this.pages = orderedIds.map((id) => {
      const page = pagesById.get(id);
      if (!page) {
        throw new Error(`Documentation page does not exist: ${id}`);
      }
      return page;
    });
  }

  page(id: string): DocumentationPage {
    const page = this.pages.find((candidate) => candidate.id === id);
    if (!page) {
      throw new Error(`Documentation page does not exist: ${id}`);
    }
    return page;
  }

  neighbors(id: string): DocumentationNeighbors {
    const index = this.pages.findIndex((page) => page.id === id);
    if (index === -1) {
      throw new Error(`Documentation page does not exist: ${id}`);
    }

    return {
      previous: this.pages[index - 1],
      next: this.pages[index + 1],
    };
  }

  llmsIndex(site: URL): string {
    const sections = DOCUMENTATION_SECTIONS.map(({ title, pages }) => {
      const links = pages
        .map((id) => this.page(id))
        .map(
          (page) =>
            `- [${page.title}](${page.markdownUrl(site)}): ${page.description}`,
        )
        .join('\n');
      return `## ${title}\n\n${links}`;
    }).join('\n\n');

    return [
      '# BoltFFI',
      '',
      '> BoltFFI generates native bindings and distributable packages for Rust libraries targeting Swift, Kotlin, Java, C#, TypeScript, and Python.',
      '',
      'Use these Markdown pages as the source of truth for BoltFFI APIs and generated target-language behavior. Start with Getting Started for a new integration, then use the page that matches the Rust API being exported.',
      '',
      sections,
      '',
      '## Complete documentation',
      '',
      `- [All BoltFFI documentation in one file](${new URL('/llms-full.txt', site)}): The complete documentation corpus for tools that prefer a single context file.`,
      '',
    ].join('\n');
  }

  llmsFull(site: URL): string {
    const documents = this.pages
      .map((page) =>
        [
          `Source: ${page.htmlUrl(site)}`,
          '',
          page.markdown().trimEnd(),
        ].join('\n'),
      )
      .join('\n\n---\n\n');

    return [
      '# BoltFFI Documentation',
      '',
      '> Complete Markdown documentation for BoltFFI.',
      '',
      `Index: ${new URL('/llms.txt', site)}`,
      '',
      documents,
      '',
    ].join('\n');
  }
}
