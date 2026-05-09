import { useState } from "react";
import { cn } from "@/lib/utils";

interface TypeMapping {
  rust: string;
  swift: string;
  kotlin: string;
  java?: string;
  csharp?: string;
  typescript?: string;
}

interface TypeTableProps {
  title: string;
  mappings: TypeMapping[];
}

const TypeTable = ({ title, mappings }: TypeTableProps) => {
  const hasJava = mappings.some(m => m.java);
  const hasCSharp = mappings.some(m => m.csharp);
  const hasTypeScript = mappings.some(m => m.typescript);
  const langs: ("Swift" | "Kotlin" | "Java" | "C#" | "TypeScript")[] = ["Swift", "Kotlin"];
  if (hasJava) langs.push("Java");
  if (hasCSharp) langs.push("C#");
  if (hasTypeScript) langs.push("TypeScript");

  const [activeLang, setActiveLang] = useState<"Swift" | "Kotlin" | "Java" | "C#" | "TypeScript">("Swift");

  const getValue = (mapping: TypeMapping) => {
    if (activeLang === "Swift") return mapping.swift;
    if (activeLang === "Kotlin") return mapping.kotlin;
    if (activeLang === "Java") return mapping.java || mapping.kotlin;
    if (activeLang === "C#") return mapping.csharp || mapping.java || mapping.kotlin;
    return mapping.typescript || mapping.kotlin;
  };

  return (
    <div className="my-3">
      <div className="flex items-center gap-3 mb-2">
        <span className="text-sm font-medium text-muted-foreground">{title}</span>
        <div className="flex items-center gap-0.5">
          {langs.map((lang) => (
            <button
              key={lang}
              onClick={() => setActiveLang(lang)}
              className={cn(
                "px-2 py-0.5 rounded text-xs font-mono transition-colors",
                activeLang === lang
                  ? "bg-primary/20 text-primary"
                  : "text-muted-foreground hover:text-foreground"
              )}
            >
              {lang}
            </button>
          ))}
        </div>
      </div>
      <div className="flex flex-wrap gap-x-6 gap-y-1">
        {mappings.map((mapping, i) => (
          <div key={i} className="flex items-center gap-2">
            <code className="text-sm font-mono text-primary">{mapping.rust}</code>
            <span className="text-muted-foreground text-sm">→</span>
            <code className="text-sm font-mono text-muted-foreground">
              {getValue(mapping)}
            </code>
          </div>
        ))}
      </div>
    </div>
  );
};

export default TypeTable;
