import type { APIRoute } from 'astro';
import { getCollection } from 'astro:content';
import { DocumentationCorpus } from '../lib/documentation';

export const GET: APIRoute = async ({ site }) => {
  const corpus = new DocumentationCorpus(await getCollection('docs'));
  return new Response(
    corpus.llmsIndex(site ?? new URL('https://boltffi.dev')),
    {
      headers: {
        'Content-Type': 'text/markdown; charset=utf-8',
      },
    },
  );
};
