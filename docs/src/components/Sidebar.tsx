import { useState } from "react";
import { cn } from "@/lib/utils";
import { 
  Layers, 
  Terminal, 
  Zap,
  Box,
  RefreshCw,
  Radio,
  AlertCircle,
  FileCode,
  Workflow,
  GraduationCap,
  Database,
  Settings,
  FlaskConical,
  ChevronDown,
  type LucideIcon
} from "lucide-react";

interface SubSection {
  id: string;
  label: string;
}

interface Section {
  id: string;
  label: string;
  icon: LucideIcon;
  children?: SubSection[];
}

interface SidebarProps {
  activeSection: string;
}

const sections: Section[] = [
  { id: "overview", label: "Overview", icon: Zap },
  { id: "getting-started", label: "Getting Started", icon: Terminal },
  { 
    id: "types", 
    label: "Types", 
    icon: Layers,
    children: [
      { id: "quick-reference", label: "Quick Reference" },
      { id: "primitives", label: "Primitives" },
      { id: "strings", label: "Strings" },
      { id: "records", label: "Records" },
      { id: "classes", label: "Classes" },
      { id: "option", label: "Option" },
      { id: "result-and-errors", label: "Result" },
      { id: "collections", label: "Collections" },
      { id: "callbacks", label: "Callbacks" },
      { id: "built-in-types", label: "Built-in Custom Types" },
    ]
  },
  { 
    id: "records", 
    label: "Records", 
    icon: Database,
    children: [
      { id: "structs", label: "Structs" },
      { id: "default-values", label: "Default Values" },
      { id: "enums", label: "Enums" },
      { id: "methods-and-constructors", label: "Methods & Constructors" },
    ]
  },
  { 
    id: "classes", 
    label: "Classes", 
    icon: Box,
    children: [
      { id: "defining-a-class", label: "Defining a Class" },
      { id: "constructors", label: "Constructors" },
      { id: "methods", label: "Methods" },
      { id: "thread-safety", label: "Thread Safety" },
      { id: "memory-management", label: "Memory Management" },
    ]
  },
  { 
    id: "functions", 
    label: "Functions", 
    icon: FileCode,
    children: [
      { id: "basic-export", label: "Basic Export" },
      { id: "parameters", label: "Parameters" },
      { id: "return-types", label: "Return Types" },
      { id: "async-functions", label: "Async Functions" },
    ]
  },
  { 
    id: "async", 
    label: "Async", 
    icon: RefreshCw,
    children: [
      { id: "how-it-works", label: "How It Works" },
      { id: "standalone-functions", label: "Standalone Functions" },
      { id: "methods", label: "Methods" },
      { id: "error-handling", label: "Error Handling" },
      { id: "cancellation", label: "Cancellation" },
      { id: "runtime", label: "Runtime" },
    ]
  },
  { 
    id: "callbacks", 
    label: "Callbacks & Traits", 
    icon: Workflow,
    children: [
      { id: "closures", label: "Closures" },
      { id: "traits", label: "Traits" },
      { id: "how-it-works", label: "How It Works" },
    ]
  },
  { 
    id: "streaming", 
    label: "Streaming", 
    icon: Radio,
    children: [
      { id: "the-ffi_stream-attribute", label: "The Attribute" },
      { id: "stream-modes", label: "Stream Modes" },
      { id: "creating-streams", label: "Creating Streams" },
      { id: "buffer-capacity", label: "Buffer Capacity" },
      { id: "stopping-streams", label: "Stopping Streams" },
    ]
  },
  { 
    id: "errors", 
    label: "Errors", 
    icon: AlertCircle,
    children: [
      { id: "supported-error-types", label: "Supported Types" },
      { id: "string-errors", label: "String Errors" },
      { id: "struct-errors", label: "Struct Errors" },
      { id: "enum-errors", label: "Enum Errors" },
      { id: "async-errors", label: "Async Errors" },
    ]
  },
  { 
    id: "custom-types", 
    label: "Custom Types", 
    icon: Layers,
    children: [
      { id: "the-custom_type-macro", label: "The Macro" },
      { id: "the-customfficonvertible-trait", label: "The Trait" },
      { id: "choosing-an-approach", label: "Choosing an Approach" },
      { id: "representation-types", label: "Representation Types" },
      { id: "containers", label: "Containers" },
    ]
  },
  { 
    id: "packaging", 
    label: "Packaging", 
    icon: Box,
    children: [
      { id: "apple-packaging", label: "Apple" },
      { id: "android-packaging", label: "Android" },
      { id: "java-packaging", label: "Java" },
      { id: "c-sharp-generation", label: "C#" },
      { id: "wasm-packaging", label: "WASM" },
    ]
  },
  { 
    id: "configuration", 
    label: "Configuration", 
    icon: Settings,
    children: [
      { id: "package-identity", label: "Package Identity" },
      { id: "apple-configuration", label: "Apple" },
      { id: "swiftpm-layouts", label: "SwiftPM Layouts" },
      { id: "android-configuration", label: "Android" },
      { id: "java-configuration", label: "Java" },
      { id: "c-sharp-configuration", label: "C#" },
      { id: "wasm-configuration", label: "WASM" },
    ]
  },
  { 
    id: "experimental", 
    label: "Experimental", 
    icon: FlaskConical,
    children: [
      { id: "enabling-experimental-features", label: "Enabling" },
      { id: "current-experimental-features", label: "Current Features" },
    ]
  },
  { id: "tutorial", label: "Tutorial", icon: GraduationCap },
];

export default function Sidebar({ activeSection }: SidebarProps) {
  const [expandedSections, setExpandedSections] = useState<Set<string>>(
    new Set([activeSection])
  );

  const toggleSection = (id: string) => {
    setExpandedSections(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const scrollToSection = (sectionId: string, subsectionId: string) => {
    window.location.href = `/docs/${sectionId}#${subsectionId}`;
  };

  return (
    <aside className="w-56 shrink-0 sticky top-14 h-[calc(100vh-3.5rem)] overflow-y-auto hidden lg:block">
      <nav className="py-6 pr-4">
        <p className="text-xs font-semibold uppercase tracking-widest text-muted-foreground mb-4">
          Documentation
        </p>
        <ul className="space-y-0.5">
          {sections.map(({ id, label, icon: Icon, children }) => {
            const isActive = activeSection === id;
            const isExpanded = expandedSections.has(id) || isActive;
            const hasChildren = children && children.length > 0;

            return (
              <li key={id}>
                <div className="flex items-center">
                  <a
                    href={`/docs/${id}`}
                    className={cn(
                      "flex items-center gap-2.5 flex-1 px-3 py-2 rounded-md text-sm transition-colors",
                      isActive
                        ? "bg-primary/10 text-primary font-medium"
                        : "text-muted-foreground hover:text-foreground hover:bg-muted/50"
                    )}
                  >
                    <Icon className="w-4 h-4 shrink-0" />
                    {label}
                  </a>
                  {hasChildren && (
                    <button
                      onClick={() => toggleSection(id)}
                      className="p-1.5 text-muted-foreground hover:text-foreground"
                    >
                      <ChevronDown 
                        className={cn(
                          "w-3.5 h-3.5 transition-transform",
                          isExpanded && "rotate-180"
                        )} 
                      />
                    </button>
                  )}
                </div>
                {hasChildren && isExpanded && (
                  <ul className="ml-6 mt-1 space-y-0.5 border-l border-border pl-3">
                    {children.map((child) => (
                      <li key={child.id}>
                        <button
                          onClick={() => scrollToSection(id, child.id)}
                          className="w-full px-2 py-1.5 rounded text-xs text-left transition-colors text-muted-foreground hover:text-foreground"
                        >
                          {child.label}
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            );
          })}
        </ul>
      </nav>
    </aside>
  );
}
