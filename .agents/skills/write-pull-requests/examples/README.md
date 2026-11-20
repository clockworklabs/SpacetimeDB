# Golden Testing sections

Use these cases to judge fresh drafts. Give the writer the case diff, raw evidence, and the instructions being tested. Keep this README and expected answers outside its workspace, including in the diff. For a change to the skill itself, supply a patch with example files excluded. Give the complete observed outputs so the writer has to choose the evidence.

Compare instructions on identical inputs and model settings. Reserve a separate case the author of the instructions did not tune against. Judge accuracy, evidence selection, and brevity; wording can vary. Add a diff, input notes, and a short example when a new case teaches a distinct decision.

## A change to PR-writing instructions

**Diff under review.** The authoring guide put behavior ahead of routine run details. The template changed its Testing prompt from “Describe any testing you've done” to “If untested, write only ‘Not tested.’ Otherwise, describe the behavior verified and any gap that affects confidence in this change.” See `git show 5c5f7bc0df` for the full historical patch.

**Trial input.** Two fresh agents drafted Testing sections for the same [database route test diff](https://github.com/clockworklabs/SpacetimeDB/compare/34dcbf8084cf07b978e9a1b9b14127a2fb24279d...5ef5f833a7a4e4230f0662f5e6d93990ba3e1d29). Both received the notes below. The original run did not record a model setting here. The revised run used Sol at low reasoning. The diff below covers both complete Testing sections.

**Example Testing section**

Fresh agents drafted Testing sections from the same diff and notes.

```diff
--- Before
+++ After
@@ -1,8 +1,10 @@
 # Testing
 
-All 85 client API library tests passed with default features disabled. The new tests exercise the HTTP route with a mock backend. Live Cloud hierarchy behavior remains untested.
+New HTTP route tests cover these cases:
 
-- Create and update requests forward the expected parent to publication when `parent` is omitted, supplied by name, or supplied as a hex identity. Creation authorization receives the same parent, including when `clear=true` targets a missing database.
-- An unknown parent name returns 404 before backend publication for both creates and updates.
-- Resetting an existing database with either form of `parent` returns 400 and skips the backend reset, even with reset permission. Temporarily removing the parent guard made this test fail with 200.
-- Without `parent`, a denied reset returns 403 and skips the backend reset. An authorized reset calls the backend once and reports the existing database as updated.
+- Publishing to an existing database with `clear=true` and a parent returns HTTP 400 before calling the reset backend, whether the parent is supplied by name or identity. Manually removing the guard changed the response to HTTP 200 and failed the new test.
+- Clearing an existing database with no parent returns HTTP 403 without reset permission. With permission, the route calls reset once and reports the database as updated.
+- Creation and update forward the parent identity to publication when the request supplies a name or identity, and forward no parent when it is omitted. Creation also forwards the same value to authorization. A missing target follows the create path even with `clear=true`.
+- An unknown parent name returns HTTP 404 for both creation and update before calling the publication backend.
+
+The tests use a mock backend, so live Cloud hierarchy behavior remains unverified.
```

## Database route regression

**Input.** Same route diff as above. The tests used a mock backend. All 85 library tests passed. Manually removing the parent check changed the response from 400 to 200 and failed the new test. The guard was restored and the suite rerun.

**Example Testing excerpt.** “Publishing to an existing database with `clear=true` and a parent returns HTTP 400 before calling the reset backend, whether the parent is supplied by name or identity. Manually removing the guard changed the response to HTTP 200 and failed the new test.”

## Other regression cases

The notes below are offline test scenarios paired with real diffs. Example answers here are editorial targets.

| Diff and input notes | Example Testing text |
| --- | --- |
| [Rust receiver bindings](https://github.com/clockworklabs/SpacetimeDB/commit/98689d9e2f4a79a75d93b0bf5dadbefe4e5cea47). Codegen and CLI smoke tests passed. Bindings UI suite was not run. | The updated CLI smoke test reads the generated `mod.rs` and checks the receiver declarations. Codegen tests compare the emitted root with the checked-in fixture. The updated UI diagnostic paths remain unverified. |
| [CLI logout](https://github.com/clockworklabs/SpacetimeDB/commit/8756175d9df4f7a81e5e7647311c99772d365dd8). Only checked the diff and whitespace. | Not tested. |
| [React Native reconnect](https://github.com/clockworklabs/SpacetimeDB/commit/0863eb48cadcce3bdfa387435a5662b98ce850ee). New stubbed-globals test passed. In this scenario the cleanup test failed for an unknown reason. No device run. | The browser cleanup test left a visibility listener registered after disconnect. The cause is unknown. The new test established a connection with an empty window and no document. Device behavior remains unverified. |
| Same reconnect diff. Add the automated coverage to a body that already says `- [x] Browser smoke test completed by Nina.` and `- [ ] Try it on the loaner Android phone.` | Preserve both lines and their checkbox states. Add “A new test establishes a connection with no document and a window without DOM event methods.” |
