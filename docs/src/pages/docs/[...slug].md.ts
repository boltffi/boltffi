import type { APIRoute, GetStaticPaths } from 'astro';
import { getCollection, type CollectionEntry } from 'astro:content';
import { DocumentationPage } from '../../lib/documentation';

interface MarkdownPageProps {
  entry: CollectionEntry<'docs'>;
}

export const getStaticPaths = (async () => {
  const entries = await getCollection('docs');
  return entries.map((entry) => ({
    params: { slug: entry.id },
    props: { entry },
  }));
}) satisfies GetStaticPaths;

export const GET: APIRoute = ({ props, site }) => {
  const { entry } = props as MarkdownPageProps;
  return new DocumentationPage(entry).markdownResponse(
    site ?? new URL('https://boltffi.dev'),
  );
};
