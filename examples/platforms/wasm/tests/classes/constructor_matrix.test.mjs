import { assert, demo } from "../support/index.mjs";

function assertMatrix(matrix, expectedVariant, expectedSummary, expectedChecksum, expectedVectorCount) {
  assert.ok(matrix);
  assert.equal(matrix.constructorVariant(), expectedVariant);
  assert.equal(matrix.summary(), expectedSummary);
  assert.equal(matrix.payloadChecksum(), expectedChecksum);
  assert.equal(matrix.vectorCount(), expectedVectorCount);
  matrix.dispose();
}

export async function run() {
  assertMatrix(demo.ConstructorCoverageMatrix.new(), "new", "default", 0, 0);
  assertMatrix(
    demo.ConstructorCoverageMatrix.withScalarMix(7, true, demo.Priority.High),
    "with_scalar_mix",
    "version=7;enabled=true;priority=high",
    0,
    0,
  );
  assertMatrix(
    demo.ConstructorCoverageMatrix.withStringAndBytes("bolt", new Uint8Array([1, 2, 3, 4])),
    "with_string_and_bytes",
    "label=bolt;bytes=4",
    10,
    4,
  );
  assertMatrix(
    demo.ConstructorCoverageMatrix.withBlittableAndRecord({ x: 1.5, y: 2.5 }, { name: "Alice", age: 31 }),
    "with_blittable_and_record",
    "origin=1.5:2.5;person=Alice#31",
    0,
    1,
  );
  assertMatrix(
    demo.ConstructorCoverageMatrix.withOptionalProfileAndCursor(
      { name: "John", age: 29, email: "john@example.com", score: 9.5 },
      "cursor-7",
    ),
    "with_optional_profile_and_cursor",
    "profile=John#29#john@example.com#9.5;cursor=cursor-7",
    0,
    2,
  );
  assertMatrix(
    demo.ConstructorCoverageMatrix.withVectorsAndPolygon(
      ["ffi", "swift"],
      [{ x: 0, y: 0 }, { x: 1, y: 1 }],
      { points: [{ x: 0, y: 0 }, { x: 2, y: 0 }, { x: 1, y: 1 }] },
    ),
    "with_vectors_and_polygon",
    "tags=ffi|swift;anchors=2;polygon=3",
    0,
    7,
  );
  assertMatrix(
    demo.ConstructorCoverageMatrix.withCollectionRecords(
      { name: "Platform", members: ["Alice", "John"] },
      { students: [{ name: "Alice", age: 20 }, { name: "John", age: 21 }] },
      { points: [{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 1, y: 1 }] },
    ),
    "with_collection_records",
    "team=Platform;members=2;students=2;polygon=3",
    0,
    7,
  );
  assertMatrix(
    demo.ConstructorCoverageMatrix.withEnumMix(
      { tag: "ByGroups", groups: [["café", "🌍"], [], ["common"]] },
      { tag: "Image", url: "https://example.com/image.png", width: 640, height: 480 },
      { title: "ship", priority: demo.Priority.Critical, completed: false },
    ),
    "with_enum_mix",
    "filter=groups:3;message=image:https://example.com/image.png#640x480;task=ship#critical",
    0,
    1,
  );
  assertMatrix(
    demo.ConstructorCoverageMatrix.withEverything(
      { name: "Alice", age: 31 },
      { street: "Main", city: "AMS", zip: "1000" },
      { name: "John", age: 29, email: "john@example.com", score: 9.5 },
      { query: "route", total: 5, nextCursor: "next-9", maxScore: 7.5 },
      new Uint8Array([4, 5, 6]),
      { tag: "ByRange", min: 1, max: 3 },
      ["alpha", "beta"],
    ),
    "with_everything",
    "person=Alice#31;city=AMS;profile=profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0;tags=alpha|beta",
    15,
    10,
  );
  const borrowedSummaryMatrix = demo.ConstructorCoverageMatrix.withEverything(
    { name: "Alice", age: 31 },
    { street: "Main", city: "AMS", zip: "1000" },
    { name: "John", age: 29, email: "john@example.com", score: 9.5 },
    { query: "route", total: 5, nextCursor: "next-9", maxScore: 7.5 },
    new Uint8Array([4, 5, 6]),
    { tag: "ByRange", min: 1, max: 3 },
    ["alpha", "beta"],
  );
  assert.equal(
    borrowedSummaryMatrix.summarizeBorrowedInputs(
      { name: "John", age: 29, email: "john@example.com", score: 9.5 },
      { query: "route", total: 5, nextCursor: "next-9", maxScore: 7.5 },
      { tag: "ByRange", min: 1, max: 3 },
    ),
    "profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0",
  );
  borrowedSummaryMatrix.dispose();

  const fallible = demo.ConstructorCoverageMatrix.tryWithPayloadAndSearchResult(
    new Uint8Array([7, 8]),
    { query: "search", total: 4, nextCursor: "cursor-4", maxScore: null },
    { tag: "ByName", name: "ali" },
  );
  assertMatrix(
    fallible,
    "try_with_payload_and_search_result",
    "query=search;cursor=cursor-4;filter=name:ali",
    15,
    6,
  );

  assert.equal(
    demo.ConstructorCoverageMatrix.tryWithPayloadAndSearchResult(
      new Uint8Array(),
      { query: "search", total: 4, nextCursor: null, maxScore: null },
      { tag: "None" },
    ),
    null,
  );
}
