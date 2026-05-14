import { assert, demo } from "../support/index.mjs";

export async function run() {
  // case:records.with_options.user_profile.some_none
  const userProfile = demo.makeUserProfile("Alice", 30, "alice@example.com", 98.5);
  assert.deepEqual(demo.echoUserProfile(userProfile), userProfile);
  assert.equal(demo.userDisplayName(userProfile), "Alice <alice@example.com>");
  assert.equal(demo.userDisplayName(demo.makeUserProfile("Bob", 22, null, null)), "Bob");

  // case:records.with_options.search_result.some_none
  const searchResult = { query: "rust ffi", total: 12, nextCursor: "cursor-1", maxScore: 0.99 };
  assert.deepEqual(demo.echoSearchResult(searchResult), searchResult);
  assert.equal(demo.hasMoreResults(searchResult), true);
  assert.equal(
    demo.hasMoreResults({ query: "rust ffi", total: 12, nextCursor: null, maxScore: null }),
    false,
  );
}
