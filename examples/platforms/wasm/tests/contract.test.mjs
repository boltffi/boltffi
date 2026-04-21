import { readFile, readdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { assert } from "./support/index.mjs";

const testsRoot = dirname(fileURLToPath(import.meta.url));
const wasmRoot = dirname(testsRoot);
const repositoryRoot = dirname(dirname(dirname(wasmRoot)));
const rustSourceRoot = join(repositoryRoot, "examples", "demo", "src");
const generatedDeclarationPath = join(wasmRoot, "dist", "demo.d.ts");

// Blocked on #203: nested Vec<Vec<isize>>/Vec<Vec<usize>> writer passes
// plain Number to setBigInt64; re-enable once the TS/WASM lowering coerces.
const unsupportedTopLevelFunctions = new Set([
  "primitives/vecs.rs::echoVecVecIsize",
  "primitives/vecs.rs::echoVecVecUsize",
]);

const unsupportedTypeMembers = new Set([
  "classes/streams.rs::EventBus::subscribeValues",
  "classes/streams.rs::EventBus::subscribePoints",
  "classes/streams.rs::EventBus::subscribeValuesBatch",
  "classes/streams.rs::EventBus::subscribeValuesCallback",
]);

const tsKeywords = new Set([
  "break",
  "case",
  "catch",
  "class",
  "const",
  "continue",
  "debugger",
  "default",
  "delete",
  "do",
  "else",
  "enum",
  "export",
  "extends",
  "false",
  "finally",
  "for",
  "function",
  "if",
  "import",
  "in",
  "instanceof",
  "new",
  "null",
  "return",
  "super",
  "switch",
  "this",
  "throw",
  "true",
  "try",
  "typeof",
  "var",
  "void",
  "while",
  "with",
  "yield",
  "let",
  "static",
  "implements",
  "interface",
  "package",
  "private",
  "protected",
  "public",
  "type",
]);

function snakeToCamel(name) {
  return name.replace(/_([a-z])/g, (_, char) => char.toUpperCase());
}

function escapeTsName(name) {
  return tsKeywords.has(name) ? `${name}_` : name;
}

function rustSignatureKey(rustFile, rustName) {
  return `${rustFile}::${escapeTsName(snakeToCamel(rustName))}`;
}

function rustMemberKey(rustFile, typeName, rustName) {
  return `${rustFile}::${typeName}::${escapeTsName(snakeToCamel(rustName))}`;
}

function generatedTypeMemberName(item, generatedSurface) {
  if (generatedSurface.classes[item.typeName]) {
    return item.rustName === "new" ? "new" : escapeTsName(item.generatedName);
  }
  if (generatedSurface.namespaces[item.typeName] && item.rustName === "new") {
    return "fromRaw";
  }
  if (generatedSurface.companions[item.typeName]) {
    return item.generatedName;
  }
  return escapeTsName(item.generatedName);
}

async function collectRustFiles(currentRoot, currentRelativePath = "") {
  const entries = await readdir(currentRoot, { withFileTypes: true });
  const nestedFiles = await Promise.all(
    entries.map(async (entry) => {
      const relativePath = currentRelativePath
        ? `${currentRelativePath}/${entry.name}`
        : entry.name;
      const fullPath = join(currentRoot, entry.name);
      if (entry.isDirectory()) {
        return collectRustFiles(fullPath, relativePath);
      }
      return entry.name.endsWith(".rs") ? [relativePath] : [];
    }),
  );
  return nestedFiles.flat().sort();
}

async function collectFiles(currentRoot, matcher, currentRelativePath = "") {
  const entries = await readdir(currentRoot, { withFileTypes: true });
  const nestedFiles = await Promise.all(
    entries.map(async (entry) => {
      const relativePath = currentRelativePath
        ? `${currentRelativePath}/${entry.name}`
        : entry.name;
      const fullPath = join(currentRoot, entry.name);
      if (entry.isDirectory()) {
        return collectFiles(fullPath, matcher, relativePath);
      }
      return matcher(relativePath) ? [relativePath] : [];
    }),
  );
  return nestedFiles.flat().sort();
}

function captureBlock(source, startIndex) {
  let depth = 1;
  let index = startIndex;
  while (index < source.length && depth > 0) {
    if (source[index] === "{") {
      depth += 1;
    } else if (source[index] === "}") {
      depth -= 1;
    }
    index += 1;
  }
  return source.slice(startIndex, index - 1);
}

function parseRustInventory(source, rustFile) {
  const topLevelFunctions = [
    ...source.matchAll(/#\[export(?:\([^\]]*\))?\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)/g),
  ].map((match) => ({ rustFile, rustName: match[1], generatedName: snakeToCamel(match[1]) }));

  const typeMembers = [];
  const traitMethods = [];

  const implPatterns = [
    { marker: /#\[export\]\s*impl\s+(\w+)\s*\{/g, exported: true },
    { marker: /#\[data\(impl\)\]\s*impl\s+(\w+)\s*\{/g, exported: true },
  ];

  implPatterns.forEach(({ marker }) => {
    [...source.matchAll(marker)].forEach((match) => {
      const typeName = match[1];
      const block = captureBlock(source, match.index + match[0].length);
      [...block.matchAll(/pub\s+(?:async\s+)?fn\s+(\w+)\s*\(/g)].forEach((methodMatch) => {
        typeMembers.push({
          rustFile,
          typeName,
          rustName: methodMatch[1],
          generatedName: snakeToCamel(methodMatch[1]),
        });
      });
    });
  });

  [...source.matchAll(/#\[export\]\s*pub\s+trait\s+(\w+)\s*\{/g)].forEach((match) => {
    const protocolName = match[1];
    const block = captureBlock(source, match.index + match[0].length);
    [...block.matchAll(/(?:async\s+)?fn\s+(\w+)\s*\(/g)].forEach((methodMatch) => {
      traitMethods.push({
        rustFile,
        protocolName,
        rustName: methodMatch[1],
        generatedName: snakeToCamel(methodMatch[1]),
      });
    });
  });

  return { topLevelFunctions, typeMembers, traitMethods };
}

function parseGeneratedSurface(source) {
  const topLevelFunctions = new Set(
    [...source.matchAll(/^export declare function (\w+)\(/gm)].map((match) => match[1]),
  );

  const classes = Object.fromEntries(
    [...source.matchAll(/^export declare class (\w+) \{([\s\S]*?)^\}/gm)].map((match) => [
      match[1],
      new Set(
        [...match[2].matchAll(/^\s+(?:static\s+)?(\w+)\(/gm)]
          .map((methodMatch) => methodMatch[1])
          .filter((name) => !name.startsWith("_")),
      ),
    ]),
  );

  const interfaces = Object.fromEntries(
    [...source.matchAll(/^export interface (\w+) \{([\s\S]*?)^\}/gm)].map((match) => [
      match[1],
      new Set([...match[2].matchAll(/^\s+(\w+)\(/gm)].map((methodMatch) => methodMatch[1])),
    ]),
  );

  const companions = Object.fromEntries(
    [...source.matchAll(/^export declare const (\w+): \{([\s\S]*?)^\};/gm)].map((match) => [
      match[1],
      new Set(
        [...match[2].matchAll(/^\s+"?(\w+)"?(?:\(|: \()/gm)].map((methodMatch) => methodMatch[1]),
      ),
    ]),
  );

  const namespaces = Object.fromEntries(
    [...source.matchAll(/^export declare namespace (\w+) \{([\s\S]*?)^\}/gm)].map((match) => [
      match[1],
      new Set(
        [...match[2].matchAll(/^\s+const (\w+): \(/gm)].map((methodMatch) => methodMatch[1]),
      ),
    ]),
  );

  const typeMembers = {};
  for (const [typeName, members] of Object.entries(classes)) {
    typeMembers[typeName] = new Set(members);
  }
  for (const [typeName, members] of Object.entries(interfaces)) {
    typeMembers[typeName] = new Set([...(typeMembers[typeName] ?? []), ...members]);
  }
  for (const [typeName, members] of Object.entries(companions)) {
    typeMembers[typeName] = new Set([...(typeMembers[typeName] ?? []), ...members]);
  }
  for (const [typeName, members] of Object.entries(namespaces)) {
    typeMembers[typeName] = new Set([...(typeMembers[typeName] ?? []), ...members]);
  }

  return { topLevelFunctions, classes, interfaces, companions, namespaces, typeMembers };
}

async function loadTestSources() {
  const testFiles = await collectFiles(testsRoot, (relativePath) => relativePath.endsWith(".mjs"));
  const relevantFiles = testFiles.filter(
    (relativePath) =>
      relativePath.endsWith(".test.mjs") &&
      relativePath !== "contract.test.mjs" &&
      !relativePath.startsWith("support/"),
  );
  const contents = await Promise.all(
    relevantFiles.map(async (relativePath) => [
      relativePath,
      await readFile(join(testsRoot, relativePath), "utf8"),
    ]),
  );
  return Object.fromEntries(contents);
}

function expectedTestPath(rustFile) {
  return rustFile.replace(/\.rs$/, ".test.mjs");
}

export async function run() {
  const rustFiles = (await collectRustFiles(rustSourceRoot)).filter((relativePath) => relativePath !== "lib.rs");
  const rustInventories = await Promise.all(
    rustFiles.map(async (relativePath) => parseRustInventory(await readFile(join(rustSourceRoot, relativePath), "utf8"), relativePath)),
  );
  const rustTopLevelFunctions = rustInventories.flatMap((inventory) => inventory.topLevelFunctions);
  const rustTypeMembers = rustInventories.flatMap((inventory) => inventory.typeMembers);
  const rustTraitMethods = rustInventories.flatMap((inventory) => inventory.traitMethods);

  const generatedSurface = parseGeneratedSurface(await readFile(generatedDeclarationPath, "utf8"));
  const testSources = await loadTestSources();
  const duplicateGeneratedTopLevelFunctionNames = [];
  const duplicateGeneratedTypeMemberNames = [];

  const generatedTopLevelFunctionBuckets = new Map();
  for (const item of rustTopLevelFunctions) {
    if (unsupportedTopLevelFunctions.has(rustSignatureKey(item.rustFile, item.rustName))) {
      continue;
    }
    const generatedName = escapeTsName(item.generatedName);
    const existing = generatedTopLevelFunctionBuckets.get(generatedName);
    if (existing) {
      duplicateGeneratedTopLevelFunctionNames.push(
        `${item.rustFile} -> ${generatedName} from ${existing} and ${item.rustName}`,
      );
    } else {
      generatedTopLevelFunctionBuckets.set(generatedName, item.rustName);
    }
  }

  const generatedMemberBuckets = new Map();
  for (const item of rustTypeMembers) {
    if (unsupportedTypeMembers.has(rustMemberKey(item.rustFile, item.typeName, item.rustName))) {
      continue;
    }
    const generatedName = generatedTypeMemberName(item, generatedSurface);
    const bucketKey = `${item.rustFile}::${item.typeName}::${generatedName}`;
    const existing = generatedMemberBuckets.get(bucketKey);
    if (existing) {
      duplicateGeneratedTypeMemberNames.push(
        `${item.rustFile} -> ${item.typeName}.${generatedName} from ${existing} and ${item.rustName}`,
      );
    } else {
      generatedMemberBuckets.set(bucketKey, item.rustName);
    }
  }

  const missingTopLevelFunctions = rustTopLevelFunctions.filter(
    (item) =>
      !unsupportedTopLevelFunctions.has(rustSignatureKey(item.rustFile, item.rustName)) &&
      !generatedSurface.topLevelFunctions.has(escapeTsName(item.generatedName)),
  );

  const missingTypeMembers = rustTypeMembers.filter(
    (item) =>
      !unsupportedTypeMembers.has(rustMemberKey(item.rustFile, item.typeName, item.rustName)) &&
      !(
        generatedSurface.typeMembers[item.typeName] &&
        generatedSurface.typeMembers[item.typeName].has(
          generatedTypeMemberName(item, generatedSurface),
        )
      ),
  );

  const missingTraitMethods = rustTraitMethods.filter(
    (item) =>
      !(generatedSurface.interfaces[item.protocolName] && generatedSurface.interfaces[item.protocolName].has(item.generatedName)),
  );

  const missingTestFiles = rustInventories
    .filter((inventory) => {
      const hasSupportedFunction = inventory.topLevelFunctions.some(
        (item) => !unsupportedTopLevelFunctions.has(rustSignatureKey(item.rustFile, item.rustName)),
      );
      const hasSupportedMember = inventory.typeMembers.some(
        (item) => !unsupportedTypeMembers.has(rustMemberKey(item.rustFile, item.typeName, item.rustName)),
      );
      return hasSupportedFunction || hasSupportedMember || inventory.traitMethods.length > 0;
    })
    .map((inventory) => inventory.topLevelFunctions[0]?.rustFile ?? inventory.typeMembers[0]?.rustFile ?? inventory.traitMethods[0]?.rustFile)
    .filter((rustFile) => !testSources[expectedTestPath(rustFile)]);

  const missingTopLevelCoverage = rustTopLevelFunctions.filter((item) => {
    if (unsupportedTopLevelFunctions.has(rustSignatureKey(item.rustFile, item.rustName))) {
      return false;
    }
    const testSource = testSources[expectedTestPath(item.rustFile)] ?? "";
    return !testSource.includes(`${escapeTsName(item.generatedName)}(`);
  });

  const missingMemberCoverage = rustTypeMembers.filter((item) => {
    if (unsupportedTypeMembers.has(rustMemberKey(item.rustFile, item.typeName, item.rustName))) {
      return false;
    }
    const testSource = testSources[expectedTestPath(item.rustFile)] ?? "";
    const generatedName = generatedTypeMemberName(item, generatedSurface);
    return !(
      testSource.includes(`.${generatedName}(`) ||
      testSource.includes(`${item.typeName}.${generatedName}(`)
    );
  });

  const missingTraitCoverage = rustTraitMethods.filter((item) => {
    const testSource = testSources[expectedTestPath(item.rustFile)] ?? "";
    return !testSource.includes(item.generatedName);
  });

  assert.equal(
    duplicateGeneratedTopLevelFunctionNames.length,
    0,
    `duplicate generated wasm top-level function names:\n${duplicateGeneratedTopLevelFunctionNames.join("\n")}`,
  );
  assert.equal(
    duplicateGeneratedTypeMemberNames.length,
    0,
    `duplicate generated wasm type member names:\n${duplicateGeneratedTypeMemberNames.join("\n")}`,
  );
  assert.equal(
    missingTopLevelFunctions.length,
    0,
    `missing generated wasm top-level functions:\n${missingTopLevelFunctions.map((item) => `${item.rustFile} -> ${item.generatedName}`).join("\n")}`,
  );
  assert.equal(
    missingTypeMembers.length,
    0,
    `missing generated wasm type members:\n${missingTypeMembers.map((item) => `${item.rustFile} -> ${item.typeName}.${item.generatedName}`).join("\n")}`,
  );
  assert.equal(
    missingTraitMethods.length,
    0,
    `missing generated wasm trait methods:\n${missingTraitMethods.map((item) => `${item.rustFile} -> ${item.protocolName}.${item.generatedName}`).join("\n")}`,
  );
  assert.equal(
    missingTestFiles.length,
    0,
    `missing wasm test files:\n${missingTestFiles.join("\n")}`,
  );
  assert.equal(
    missingTopLevelCoverage.length,
    0,
    `missing wasm top-level coverage:\n${missingTopLevelCoverage.map((item) => `${item.rustFile} -> ${item.generatedName}`).join("\n")}`,
  );
  assert.equal(
    missingMemberCoverage.length,
    0,
    `missing wasm member coverage:\n${missingMemberCoverage.map((item) => `${item.rustFile} -> ${item.typeName}.${item.generatedName}`).join("\n")}`,
  );
  assert.equal(
    missingTraitCoverage.length,
    0,
    `missing wasm trait coverage:\n${missingTraitCoverage.map((item) => `${item.rustFile} -> ${item.protocolName}.${item.generatedName}`).join("\n")}`,
  );
}
