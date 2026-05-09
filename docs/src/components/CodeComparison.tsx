import { useState, useMemo } from "react";
import { cn } from "@/lib/utils";
import { Check, Copy } from "lucide-react";
import Prism from "prismjs";
import "prismjs/components/prism-rust";
import "prismjs/components/prism-swift";
import "prismjs/components/prism-kotlin";
import "prismjs/components/prism-typescript";
import "prismjs/components/prism-java";
import "prismjs/components/prism-csharp";

interface CodeComparisonProps {
  rust: string;
  swift: string;
  kotlin: string;
  java?: string;
  csharp?: string;
  typescript?: string;
  title?: string;
}

function highlight(code: string, lang: string): string {
  const grammar = Prism.languages[lang];
  if (grammar) return Prism.highlight(code, grammar, lang);
  return code;
}

const CodeComparison = ({ rust, swift, kotlin, java, csharp, typescript, title }: CodeComparisonProps) => {
  const [activeLang, setActiveLang] = useState<"Swift" | "Kotlin" | "Java" | "C#" | "TypeScript">("Swift");
  const [copiedSide, setCopiedSide] = useState<"left" | "right" | null>(null);

  const bindings: Record<string, string> = { Swift: swift, Kotlin: kotlin };
  const langMap: Record<string, string> = { Swift: "swift", Kotlin: "kotlin" };
  const availableLangs: ("Swift" | "Kotlin" | "Java" | "C#" | "TypeScript")[] = ["Swift", "Kotlin"];

  if (java) {
    bindings.Java = java;
    langMap.Java = "java";
    availableLangs.push("Java");
  }

  if (csharp) {
    bindings["C#"] = csharp;
    langMap["C#"] = "csharp";
    availableLangs.push("C#");
  }

  if (typescript) {
    bindings.TypeScript = typescript;
    langMap.TypeScript = "typescript";
    availableLangs.push("TypeScript");
  }

  const rustHighlighted = useMemo(() => highlight(rust, "rust"), [rust]);
  const bindingHighlighted = useMemo(
    () => highlight(bindings[activeLang], langMap[activeLang]),
    [activeLang, swift, kotlin, java, csharp, typescript]
  );

  const handleCopy = (code: string, side: "left" | "right") => {
    navigator.clipboard.writeText(code);
    setCopiedSide(side);
    setTimeout(() => setCopiedSide(null), 2000);
  };

  return (
    <div className="my-6">
      {title && (
        <div className="flex items-center gap-2 mb-3">
          <div className="h-px flex-1 bg-border" />
          <span className="text-xs font-mono text-muted-foreground uppercase tracking-widest">{title}</span>
          <div className="h-px flex-1 bg-border" />
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-0 rounded-lg border border-border overflow-hidden bg-[hsl(var(--code-bg))]">
        <div className="relative group border-b lg:border-b-0 lg:border-r border-border min-w-0">
          <div className="flex items-center justify-between px-4 py-2 border-b border-border bg-muted/30 h-[42px]">
            <span className="text-xs font-mono text-primary font-medium">Rust</span>
            <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-wider">Source</span>
          </div>
          <div className="overflow-x-auto scrollbar-thin not-prose">
            <pre className="p-4 text-[13px] font-mono leading-relaxed text-foreground whitespace-pre !m-0 !bg-transparent !border-0" style={{ margin: 0, background: 'transparent', border: 'none' }}>
              <code className="!bg-transparent !border-0 !p-0" style={{ background: 'transparent', border: 'none', padding: 0 }} dangerouslySetInnerHTML={{ __html: rustHighlighted }} />
            </pre>
          </div>
          <button
            onClick={() => handleCopy(rust, "left")}
            className="absolute top-2 right-10 p-1.5 rounded-md bg-muted/50 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity hover:text-foreground"
          >
            {copiedSide === "left" ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
          </button>
        </div>

        <div className="relative group min-w-0">
          <div className="flex items-center justify-between px-4 py-2 border-b border-border bg-muted/30 h-[42px]">
            <div className="flex items-center gap-1">
              {availableLangs.map((lang) => (
                <button
                  key={lang}
                  onClick={() => setActiveLang(lang)}
                  className={cn(
                    "px-2.5 py-1 rounded text-xs font-mono transition-colors",
                    activeLang === lang
                      ? "bg-primary text-primary-foreground"
                      : "text-muted-foreground hover:text-foreground hover:bg-muted/50"
                  )}
                >
                  {lang}
                </button>
              ))}
            </div>
            <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-wider hidden sm:block">Generated</span>
          </div>
          <div className="overflow-x-auto scrollbar-thin not-prose">
            <pre className="p-4 text-[13px] font-mono leading-relaxed text-foreground whitespace-pre !m-0 !bg-transparent !border-0" style={{ margin: 0, background: 'transparent', border: 'none' }}>
              <code className="!bg-transparent !border-0 !p-0" style={{ background: 'transparent', border: 'none', padding: 0 }} dangerouslySetInnerHTML={{ __html: bindingHighlighted }} />
            </pre>
          </div>
          <button
            onClick={() => handleCopy(bindings[activeLang], "right")}
            className="absolute top-2 right-3 p-1.5 rounded-md bg-muted/50 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity hover:text-foreground"
          >
            {copiedSide === "right" ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>
    </div>
  );
};

export default CodeComparison;
