# Writing pull requests

Read this guide when creating or updating a PR. Use the repository's [PR template](../.github/pull_request_template.md) and preserve every section when updating an existing PR.

## Verified commits

Verified commits are required: a PR cannot be merged without them. Enable commit signing before creating commits for a PR, and ensure GitHub recognizes the commits as **Verified**. A local signature alone does not establish that GitHub can verify it.

If you cannot create verified commits, alert the user promptly, explicitly tell them that the PR cannot be merged without verified commits, and help them set up commit signing and GitHub verification. Do not silently proceed with unverified commits or present the PR as ready to merge. After setup, confirm that GitHub marks the PR's commits as **Verified**.

## Preserve and complete the template

No section of the PR template may be removed. PRs that remove any template section will be rejected. Populate every section faithfully, following its instructions and describing the actual change; do not leave placeholders or invent information. Where a section does not apply, say so explicitly and follow any wording prescribed by the template.

## Writing the testing section

Describe the automated or manual testing performed and what behavior it covers, including relevant existing test coverage. Include important gaps in that coverage.

If no testing was performed, write “Not tested.” Do not justify the absence of testing or pad the section with routine checks. Reviewing the diff or instructions, checking whitespace or links, and verifying commit signatures are not substitutes for testing the change.

### Connect testing to the change

- Start from the change's intended outcome and the important ways it could fail. Explain how testing addresses those concerns, including relevant existing coverage.
- Usually, the strongest evidence is how automated tests were extended or adapted, or why existing tests cover the change. Manual tests and successful use can also provide evidence.
- State what the testing establishes. A same-client compilation test supports source compatibility; a client/server test supports actual interaction. Neither proves the other.

The connection between the testing and the intended outcome should be clear from the wording. It doesn't need a separate justification after every bullet.

### Content

- Distinguish updated tests, added tests, and relevant existing coverage. Use short bullets; omit categories that don't apply.
- Describe what each test verifies. Name the inputs or API when needed to make the claim clear: incorrect reducer arguments, generated bindings, or the built package.
- Explain what coverage the change adds. A new test may exercise previously tested behavior through a new path; don't imply the behavior was previously untested.
- Make compatibility claims precise. The same client against old and new bindings is different from old bindings against a new SDK.
- Include existing integration coverage when it helps explain why the change is covered. Inspect it before claiming coverage is missing or proposing more tests.
- Write assuming CI passes. Don't include a running log of local test execution or say CI tests still need to be run for the PR. This does not justify claiming a known failure passes.
- Label manual testing as manual and state the behavior exercised and the result.
- Keep caveats focused on gaps that affect confidence in the change, not a routine list of what tests don't do.

### Wording and detail

Brevity comes from removing unnecessary detail, not replacing concrete information with vague labels.

- Use plain language and concrete actions. Each bullet should describe a distinct piece of coverage.
- Describe observable behavior instead of labels such as "runtime checks," "metadata coverage," or "strengthened tests."
- Be precise about what runs. Local assertions on imported bindings are not the same as a client connecting to a server. State what is checked rather than calling both "running a client."
- Describe each piece of coverage once. Compiling a client and checking its types often describe the same test.
- Omit routine mechanics, commands, counts, and assertion details unless they materially help the reviewer.
- Scale the section to the change. Don't turn it into an inventory of every assertion or an explicit question-and-answer checklist.

### Wording examples

| Vague or misleading | Concrete |
| --- | --- |
| Strengthened codegen tests. | Updated codegen tests to check names and aliases using generated bindings instead of searching generated text. |
| Added compile-time checks. | Tests that incorrect reducer arguments fail compilation. |
| Generated-client runtime checks. | Tests that generated tables produce the expected subscription SQL. |
| Compatibility tests. | Tests that the same client compiles against old and new bindings, including existing imports, types, and aliases. |
| Package and browser checks. | Tests that the new declaration API works with ESM and CommonJS imports and browser builds. |
| End-to-end coverage. | Existing integration tests connect to a server, subscribe, call reducers, and check received rows. |

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
