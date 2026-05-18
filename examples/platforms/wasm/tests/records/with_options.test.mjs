import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.with_options.user_profile.should_make_with_present_options");
  const userProfile = demo.makeUserProfile("Alice", 30, "alice@example.com", 98.5);
  globalThis.demoCase("case:records.with_options.user_profile.should_roundtrip_present_options");
  assert.deepEqual(demo.echoUserProfile(userProfile), userProfile);
  globalThis.demoCase("case:records.with_options.user_profile.should_display_email_when_present");
  assert.equal(demo.userDisplayName(userProfile), "Alice <alice@example.com>");
  globalThis.demoCase("case:records.with_options.user_profile.should_make_with_absent_options");
  const userWithoutEmail = demo.makeUserProfile("Bob", 22, null, null);
  globalThis.demoCase("case:records.with_options.user_profile.should_roundtrip_absent_options");
  assert.deepEqual(demo.echoUserProfile(userWithoutEmail), userWithoutEmail);
  globalThis.demoCase("case:records.with_options.user_profile.should_roundtrip_mixed_options");
  const userMixedOptions = demo.makeUserProfile("Cleo", 27, "cleo@example.com", null);
  assert.deepEqual(demo.echoUserProfile(userMixedOptions), userMixedOptions);
  globalThis.demoCase("case:records.with_options.user_profile.should_roundtrip_utf8_optional_string");
  const userUtf8 = demo.makeUserProfile("Élodie", 31, "élodie@café.example", 88.25);
  assert.deepEqual(demo.echoUserProfile(userUtf8), userUtf8);
  globalThis.demoCase("case:records.with_options.user_profile.should_display_name_when_email_absent");
  assert.equal(demo.userDisplayName(userWithoutEmail), "Bob");

  globalThis.demoCase("case:records.with_options.search_result.should_roundtrip_present_options");
  const searchResult = { query: "rust ffi", total: 12, nextCursor: "cursor-1", maxScore: 0.99 };
  assert.deepEqual(demo.echoSearchResult(searchResult), searchResult);
  globalThis.demoCase("case:records.with_options.search_result.should_roundtrip_absent_options");
  const searchResultAbsent = { query: "rust ffi", total: 0, nextCursor: null, maxScore: null };
  assert.deepEqual(demo.echoSearchResult(searchResultAbsent), searchResultAbsent);
  globalThis.demoCase("case:records.with_options.search_result.should_report_more_results_when_cursor_present");
  assert.equal(demo.hasMoreResults(searchResult), true);
  globalThis.demoCase("case:records.with_options.search_result.should_report_no_more_results_without_cursor");
  assert.equal(
    demo.hasMoreResults({ query: "rust ffi", total: 12, nextCursor: null, maxScore: null }),
    false,
  );
}
