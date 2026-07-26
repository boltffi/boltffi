import type {
  Expression,
  ExpressionStatement,
  Literal,
  ObjectExpression,
  Program,
  TemplateLiteral,
} from 'estree';
import type {
  Code,
  InlineCode,
  Link,
  Paragraph,
  Root,
  RootContent,
  Strong,
  Table,
  TableCell,
  TableRow,
  Text,
} from 'mdast';
import type {
  MdxJsxAttribute,
  MdxJsxAttributeValueExpression,
  MdxJsxFlowElement,
} from 'mdast-util-mdx-jsx';
import remarkGfm from 'remark-gfm';
import remarkMdx from 'remark-mdx';
import remarkParse from 'remark-parse';
import remarkStringify from 'remark-stringify';
import { unified } from 'unified';
import { visit } from 'unist-util-visit';

interface CodeLanguage {
  property: string;
  label: string;
  fence: string;
}

interface TypeLanguage {
  property: string;
  label: string;
}

interface TypeMapping {
  rust: string;
  swift: string;
  kotlin: string;
  java?: string;
  csharp?: string;
  typescript?: string;
}

const CODE_LANGUAGES: readonly CodeLanguage[] = [
  { property: 'rust', label: 'Rust', fence: 'rust' },
  { property: 'swift', label: 'Swift', fence: 'swift' },
  { property: 'kotlin', label: 'Kotlin', fence: 'kotlin' },
  { property: 'java', label: 'Java', fence: 'java' },
  { property: 'csharp', label: 'C#', fence: 'csharp' },
  { property: 'typescript', label: 'TypeScript', fence: 'typescript' },
  { property: 'python', label: 'Python', fence: 'python' },
];

const TYPE_LANGUAGES: readonly TypeLanguage[] = [
  { property: 'rust', label: 'Rust' },
  { property: 'swift', label: 'Swift' },
  { property: 'kotlin', label: 'Kotlin' },
  { property: 'java', label: 'Java' },
  { property: 'csharp', label: 'C#' },
  { property: 'typescript', label: 'TypeScript' },
];

export class DocumentationMarkdown {
  static render(source: string): string {
    const processor = unified()
      .use(remarkParse)
      .use(remarkMdx)
      .use(remarkGfm)
      .use(remarkStringify, {
        bullet: '-',
        fences: true,
        listItemIndent: 'one',
      });
    const document = new MdxDocument(processor.parse(source) as Root);
    return processor.stringify(document.toMarkdown()).trimEnd() + '\n';
  }
}

class MdxDocument {
  constructor(private readonly root: Root) {}

  toMarkdown(): Root {
    const transformed: Root = {
      ...this.root,
      children: this.root.children.flatMap((node) => this.transform(node)),
    };

    visit(transformed, 'link', (node: Link) => {
      node.url = node.url.replace(/^\/docs\/([^#]+)(#.*)?$/, '/docs/$1.md$2');
    });

    return transformed;
  }

  private transform(node: RootContent): RootContent[] {
    if (node.type === 'mdxjsEsm') {
      return [];
    }

    if (node.type !== 'mdxJsxFlowElement') {
      return [node];
    }

    const component = node as MdxJsxFlowElement;
    if (component.name === 'CodeComparison') {
      return new CodeComparison(component).toMarkdown();
    }

    if (component.name === 'TypeTable') {
      return new TypeTable(component).toMarkdown();
    }

    return [node];
  }
}

class CodeComparison {
  private readonly attributes: MdxAttributes;

  constructor(component: MdxJsxFlowElement) {
    this.attributes = new MdxAttributes(component);
  }

  toMarkdown(): RootContent[] {
    const title = this.attributes.string('title');
    const titleNodes = title ? [MarkdownNodes.label(title)] : [];
    const examples = CODE_LANGUAGES.flatMap(({ property, label, fence }) => {
      const code = this.attributes.string(property);
      return code
        ? [MarkdownNodes.label(label), MarkdownNodes.code(fence, code)]
        : [];
    });

    return [...titleNodes, ...examples];
  }
}

class TypeTable {
  private readonly attributes: MdxAttributes;

  constructor(component: MdxJsxFlowElement) {
    this.attributes = new MdxAttributes(component);
  }

  toMarkdown(): RootContent[] {
    const title = this.attributes.string('title');
    const mappings = this.attributes.typeMappings('mappings');
    const titleNodes = title ? [MarkdownNodes.label(title)] : [];
    const header = MarkdownNodes.tableRow(
      TYPE_LANGUAGES.map(({ label }) => MarkdownNodes.tableText(label)),
    );
    const rows = mappings.map((mapping) =>
      MarkdownNodes.tableRow(
        TYPE_LANGUAGES.map(({ property }) =>
          MarkdownNodes.tableCode(mapping[property as keyof TypeMapping] ?? '—'),
        ),
      ),
    );
    const table: Table = {
      type: 'table',
      align: TYPE_LANGUAGES.map(() => null),
      children: [header, ...rows],
    };

    return [...titleNodes, table];
  }
}

class MdxAttributes {
  private readonly attributes: ReadonlyMap<string, MdxJsxAttribute>;

  constructor(component: MdxJsxFlowElement) {
    this.attributes = new Map(
      component.attributes
        .filter((attribute): attribute is MdxJsxAttribute => attribute.type === 'mdxJsxAttribute')
        .map((attribute) => [attribute.name, attribute]),
    );
  }

  string(name: string): string | undefined {
    const attribute = this.attributes.get(name);
    if (!attribute || attribute.value == null) {
      return undefined;
    }

    if (typeof attribute.value === 'string') {
      return attribute.value || undefined;
    }

    return EstreeValue.from(attribute.value, name).string();
  }

  typeMappings(name: string): TypeMapping[] {
    const attribute = this.attributes.get(name);
    if (!attribute || typeof attribute.value === 'string' || attribute.value == null) {
      throw new Error(`Expected ${name} to be a static array expression`);
    }

    return EstreeValue.from(attribute.value, name).typeMappings();
  }
}

class EstreeValue {
  private constructor(
    private readonly expression: Expression,
    private readonly attributeName: string,
  ) {}

  static from(value: MdxJsxAttributeValueExpression, attributeName: string): EstreeValue {
    const program = value.data?.estree as Program | undefined;
    const statement = program?.body[0] as ExpressionStatement | undefined;
    if (
      program?.body.length !== 1
      || statement?.type !== 'ExpressionStatement'
    ) {
      throw new Error(`Expected ${attributeName} to contain one static expression`);
    }

    return new EstreeValue(statement.expression, attributeName);
  }

  string(): string {
    if (this.expression.type === 'Literal') {
      return this.literalString(this.expression);
    }

    if (this.expression.type === 'TemplateLiteral') {
      return this.templateString(this.expression);
    }

    throw new Error(`Expected ${this.attributeName} to be a static string`);
  }

  typeMappings(): TypeMapping[] {
    if (this.expression.type !== 'ArrayExpression') {
      throw new Error(`Expected ${this.attributeName} to be an array`);
    }

    return this.expression.elements.map((element) => {
      if (!element || element.type !== 'ObjectExpression') {
        throw new Error(`Expected every ${this.attributeName} item to be an object`);
      }

      return this.typeMapping(element);
    });
  }

  private templateString(expression: TemplateLiteral): string {
    const [quasi] = expression.quasis;
    if (expression.expressions.length !== 0 || expression.quasis.length !== 1 || !quasi) {
      throw new Error(`Expected ${this.attributeName} to be a static template string`);
    }

    const value = quasi.value.cooked;
    if (value == null) {
      throw new Error(`Could not read ${this.attributeName} as a template string`);
    }

    return value;
  }

  private literalString(expression: Literal): string {
    if (typeof expression.value !== 'string') {
      throw new Error(`Expected ${this.attributeName} to be a string`);
    }

    return expression.value;
  }

  private typeMapping(expression: ObjectExpression): TypeMapping {
    const values = new Map(
      expression.properties.map((property) => this.typeMappingProperty(property)),
    );
    const rust = values.get('rust');
    const swift = values.get('swift');
    const kotlin = values.get('kotlin');

    if (!rust || !swift || !kotlin) {
      throw new Error(`Expected ${this.attributeName} items to define rust, swift, and kotlin`);
    }

    return {
      rust,
      swift,
      kotlin,
      java: values.get('java'),
      csharp: values.get('csharp'),
      typescript: values.get('typescript'),
    };
  }

  private typeMappingProperty(property: ObjectExpression['properties'][number]): [string, string] {
    if (
      property.type !== 'Property'
      || property.computed
      || property.key.type !== 'Identifier'
      || property.value.type !== 'Literal'
    ) {
      throw new Error(`Expected ${this.attributeName} items to contain static string properties`);
    }

    return [
      property.key.name,
      this.literalString(property.value),
    ];
  }
}

class MarkdownNodes {
  static label(value: string): Paragraph {
    const text: Text = { type: 'text', value };
    const strong: Strong = { type: 'strong', children: [text] };
    return { type: 'paragraph', children: [strong] };
  }

  static code(language: string, value: string): Code {
    return { type: 'code', lang: language, value };
  }

  static tableRow(children: TableCell[]): TableRow {
    return { type: 'tableRow', children };
  }

  static tableText(value: string): TableCell {
    const text: Text = { type: 'text', value };
    return { type: 'tableCell', children: [text] };
  }

  static tableCode(value: string): TableCell {
    const code: InlineCode = { type: 'inlineCode', value };
    return { type: 'tableCell', children: [code] };
  }
}
