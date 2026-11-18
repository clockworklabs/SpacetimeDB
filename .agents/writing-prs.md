# Writing pull requests

## Verified commits

Verified commits are required: a PR cannot be merged without them. Enable commit signing before creating commits for a PR, and ensure GitHub recognizes the commits as **Verified**. A local signature alone does not establish that GitHub can verify it.

If you cannot create verified commits, alert the user promptly, explicitly tell them that the PR cannot be merged without verified commits, and help them set up commit signing and GitHub verification. Do not silently proceed with unverified commits or present the PR as ready to merge. After setup, confirm that GitHub marks the PR's commits as **Verified**.

## Write for the reviewer

Use the [PR template](../.github/pull_request_template.md). Keep and complete every section, including the prescribed wording for inapplicable sections. Replace placeholders with facts from the change. Lead the description with what changes and why it matters. For a test-only PR, identify the behavior being protected. Reserve the evidence for Testing.

On an existing PR, work from the current body. Edit only the requested lines and preserve every section, the author's wording elsewhere, and the state of their checkboxes.

## Testing

If no behavioral testing was performed, the entire Testing section is the single sentence “Not tested.” Reviewing the patch, checking whitespace, or verifying signatures alone counts as untested.

Otherwise, describe the observable behavior and the evidence for it. Lead with the failure that matters most. Group related cases by the claim they establish. For example, several inputs exercising the same fallback usually need one sentence. Give a separate bullet when it answers a different reviewer concern.

Say which coverage is new, updated, or already present when that distinction matters. Inspect existing coverage before making claims about it. A manual experiment belongs here when its result tells the reviewer something specific. Label it as manual and give that result.

Routine suite status (“all tests passed”), pass counts, commands, reruns, and restoration of temporary edits belong in the author handoff. In the PR, describe each distinct behavior once. Introduce a group of new or updated tests together and let each sentence name the behavior directly. Include an execution detail in the PR when it explains a known failure, helps reproduce a relevant result, or changes the conclusion a reviewer can draw. Report known failures.

State the boundary of the evidence when it matters. Name the behavior still unverified and any known barrier. A test that compiles a client against two versions of generated bindings establishes source compatibility. A claim about exchanging messages with a server needs evidence from an actual interaction.

### Example of selecting evidence

Working notes: 38 tests passed. The test clock reached the expiry boundary. Removing the expiry comparison let an expired credential through and failed the test. Restored the comparison and reran the suite.

Testing excerpt: “A new test advances the clock to the expiry boundary and checks that the credential is rejected. Manually removing the expiry comparison let the credential through and made the test fail.”

### Example: generated bindings

Updated:

- Codegen snapshots for the new bindings format.
- Existing codegen tests to check table and function names and aliases using generated bindings instead of searching generated text.

Added:

- Tests that the same client compiles against old and new bindings, covering existing imports, types, builders, callbacks, and aliases.
- Tests that clients compile using generated submodule types, tables, reducers, and procedures.
- Tests that incorrect reducer arguments fail compilation with the new bindings.
- Tests that the new declaration API works with ESM and CommonJS imports and browser builds.

Existing server integration tests generate bindings and check connections, subscriptions, reducer calls, received rows, and query filters and joins.
