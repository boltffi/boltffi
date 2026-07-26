import { useEffect, useRef, useState } from 'react';
import {
  Bot,
  Check,
  CircleAlert,
  Copy,
  FileText,
  LoaderCircle,
} from 'lucide-react';
import { Button } from './ui/button';

interface DocsLlmActionsProps {
  title: string;
  markdownUrl: string;
  sourceUrl: string;
  llmsIndexUrl: string;
}

type CopyState = 'idle' | 'loading' | 'copied' | 'failed';

const COPY_PRESENTATION: Record<
  CopyState,
  { label: string; icon: typeof Copy }
> = {
  idle: { label: 'Copy for LLM', icon: Copy },
  loading: { label: 'Preparing', icon: LoaderCircle },
  copied: { label: 'Copied', icon: Check },
  failed: { label: 'Copy failed', icon: CircleAlert },
};

export default function DocsLlmActions({
  title,
  markdownUrl,
  sourceUrl,
  llmsIndexUrl,
}: DocsLlmActionsProps) {
  const [copyState, setCopyState] = useState<CopyState>('idle');
  const resetTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const presentation = COPY_PRESENTATION[copyState];
  const CopyIcon = presentation.icon;

  useEffect(
    () => () => {
      if (resetTimer.current) {
        clearTimeout(resetTimer.current);
      }
    },
    [],
  );

  const copyForLlm = async () => {
    setCopyState('loading');

    try {
      const response = await fetch(markdownUrl, {
        headers: { Accept: 'text/markdown' },
      });
      if (!response.ok) {
        throw new Error(`Markdown request failed with ${response.status}`);
      }

      const markdown = await response.text();
      const prompt = [
        `Help me use BoltFFI for ${title}.`,
        '',
        'Use the documentation below as the source of truth. Preserve its Rust API, type mappings, and generated target-language behavior. If this page does not cover the question, consult the BoltFFI documentation index before making assumptions.',
        '',
        `Source: ${sourceUrl}`,
        `Documentation index: ${llmsIndexUrl}`,
        '',
        '---',
        '',
        markdown.trim(),
      ].join('\n');

      await navigator.clipboard.writeText(prompt);
      setCopyState('copied');
    } catch {
      setCopyState('failed');
    }

    resetTimer.current = setTimeout(() => setCopyState('idle'), 2400);
  };

  return (
    <div className="not-prose mb-8 flex flex-col gap-3 rounded-lg border border-border bg-card/40 p-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="flex min-w-0 items-center gap-3">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
          <Bot className="size-4" aria-hidden="true" />
        </div>
        <div className="min-w-0">
          <p className="text-sm font-medium text-foreground">Use this page with an LLM</p>
          <p className="text-xs text-muted-foreground">Clean Markdown without the site chrome</p>
        </div>
      </div>
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="sm" asChild className="flex-1 sm:flex-none">
          <a href={markdownUrl} type="text/markdown">
            <FileText aria-hidden="true" />
            Markdown
          </a>
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="flex-1 sm:flex-none"
          disabled={copyState === 'loading'}
          onClick={copyForLlm}
        >
          <CopyIcon
            className={copyState === 'loading' ? 'animate-spin' : undefined}
            aria-hidden="true"
          />
          <span aria-live="polite">{presentation.label}</span>
        </Button>
      </div>
    </div>
  );
}
