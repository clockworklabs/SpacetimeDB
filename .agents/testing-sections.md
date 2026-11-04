# Writing testing sections

A PR's testing section should explain what verification gives the reviewer confidence that the change works as intended and avoids bugs. The goal is evidence for correctness, not a list of testing activities.

## Choose the evidence

- Start from the change's intended outcome and the important ways it could fail. Identify what verification addresses those concerns, including existing coverage.
- Usually, the strongest evidence is how automated tests were extended or adapted, or why existing tests cover the change. Manual tests, successful use, and other verification can also provide evidence.
- State what the evidence establishes. A same-client compilation test supports source compatibility; a client/server test supports actual interaction. Neither proves the other.
- For documentation and instructions, look for evidence that readers can accomplish the intended task. Consistency checks and working links are useful, but don't establish that the content is effective.
- Include important remaining uncertainty and explain any barriers to testing it. Distinguish "not tested yet" from "cannot be tested"; don't invent a reason or imply impossibility.

The connection between the evidence and the intended outcome should be clear from the wording. It doesn't need a separate justification after every bullet.

## Content

- Distinguish updated tests, added tests, and relevant existing coverage. Use short bullets; omit categories that don't apply.
- Describe what each test or other check verifies. Name the inputs or API when needed to make the claim clear: incorrect reducer arguments, generated bindings, or the built package.
- Explain what coverage the change adds. A new test may exercise previously tested behavior through a new path; don't imply the behavior was previously untested.
- Make compatibility claims precise. The same client against old and new bindings is different from old bindings against a new SDK.
- Include existing integration coverage when it helps explain why the change is covered. Inspect it before claiming coverage is missing or proposing more tests.
- Write assuming CI passes. Don't include a running log of local test execution or say CI tests still need to be run for the PR. This does not justify claiming a known failure passes.
- Label manual verification as manual and state what was checked and the result.
- Keep caveats focused on gaps that affect confidence in the change, not a routine list of what tests don't do.

## Wording and detail

Brevity comes from removing unnecessary detail, not replacing concrete information with vague labels.

- Use plain language and concrete actions. Each bullet should describe a distinct piece of coverage.
- Describe observable behavior instead of labels such as "runtime checks," "metadata coverage," or "strengthened tests."
- Be precise about what runs. Local assertions on imported bindings are not the same as a client connecting to a server. State what is checked rather than calling both "running a client."
- Describe each piece of coverage once. Compiling a client and checking its types often describe the same test.
- Omit routine mechanics, commands, counts, and assertion details unless they materially help the reviewer.
- Scale the section to the change. Don't turn it into an inventory of every assertion or an explicit question-and-answer checklist.

## Wording examples

| Vague or misleading | Concrete |
| --- | --- |
| Strengthened codegen tests. | Updated codegen tests to check names and aliases using generated bindings instead of searching generated text. |
| Added compile-time checks. | Tests that incorrect reducer arguments fail compilation. |
| Generated-client runtime checks. | Tests that generated tables produce the expected subscription SQL. |
| Compatibility tests. | Tests that the same client compiles against old and new bindings, including existing imports, types, and aliases. |
| Package and browser checks. | Tests that the new declaration API works with ESM and CommonJS imports and browser builds. |
| End-to-end coverage. | Existing integration tests connect to a server, subscribe, call reducers, and check received rows. |

## Example: generated bindings

Updated:

- Codegen snapshots for the new bindings format.
- Existing codegen tests to check table and function names and aliases using generated bindings instead of searching generated text.

Added:

- Tests that the same client compiles against old and new bindings, covering existing imports, types, builders, callbacks, and aliases.
- Tests that clients compile using generated submodule types, tables, reducers, and procedures.
- Tests that incorrect reducer arguments fail compilation with the new bindings.
- Tests that the new declaration API works with ESM and CommonJS imports and browser builds.

Existing server integration tests generate bindings and check connections, subscriptions, reducer calls, received rows, and query filters and joins.

## Example: writing instructions

The guidance comes from iterating on [PR #6404's testing section](https://github.com/clockworklabs/SpacetimeDB/pull/6404) until the reviewer accepted it. The examples capture those concrete improvements. We haven't yet tested whether a developer or agent using this guide alone produces a good section.

This gives evidence that the writing approach helped in practice and makes its limitation clear. "Reviewed the document and checked its links" would describe activity without addressing whether the instructions help someone write a better testing section.
