---
name: write-pull-requests
description: Prepare verified commits and write or update GitHub pull request descriptions for SpacetimeDB.
---

# Writing pull requests

When preparing commits, use the signing requirements below. When drafting or editing a description, use the template and the writing guidance.

## Verified commits

Verified commits are required: a PR cannot be merged without them. Enable commit signing before creating commits for a PR, and ensure GitHub recognizes the commits as **Verified**. A local signature alone does not establish that GitHub can verify it.

If you cannot create verified commits, alert the user promptly, explicitly tell them that the PR cannot be merged without verified commits, and help them set up commit signing and GitHub verification. Do not silently proceed with unverified commits or present the PR as ready to merge. After setup, confirm that GitHub marks the PR's commits as **Verified**.

## Write for the reviewer

Use the [PR template](../../../.github/pull_request_template.md). Keep and complete every section, including the prescribed wording for inapplicable sections. Replace placeholders with facts from the change. Lead the description with what changes and why it matters. For a test-only PR, identify the behavior being protected. Reserve the evidence for Testing.

On an existing PR, work from the current body. Edit only the requested lines and preserve every section, the author's wording elsewhere, and the state of their checkboxes.

## Testing

Running the changed workflow on a representative input and inspecting its output is behavioral testing, including for changes to authoring instructions. Use the recorded output as evidence.

If no behavioral testing was performed, the entire Testing section is the single sentence “Not tested.” Reviewing the patch, checking whitespace, or verifying signatures alone counts as untested.

Otherwise, describe the observable behavior and the evidence for it. Lead with the failure that matters most. Group related cases by the claim they establish. For example, several inputs exercising the same fallback usually need one sentence. Give a separate bullet when it answers a different reviewer concern.

Say which coverage is new, updated, or already present when that distinction matters. Inspect existing coverage before making claims about it. A manual experiment belongs here when its result tells the reviewer something specific. Label it as manual and give that result.

Routine suite status (“all tests passed”), pass counts, commands, reruns, and restoration of temporary edits belong in the author handoff. In the PR, describe each distinct behavior once. Introduce a group of new or updated tests together and let each sentence name the behavior directly. Include an execution detail in the PR when it explains a known failure, helps reproduce a relevant result, or changes the conclusion a reviewer can draw. Report known failures.

State the boundary of the evidence when it matters. Name the behavior still unverified and any known barrier. A test that compiles a client against two versions of generated bindings establishes source compatibility. A claim about exchanging messages with a server needs evidence from an actual interaction.

When generated text is the behavior under review, let a reviewer see the change. Introduce the sample task in a phrase. Quote the shortest useful before/after pair from actual runs on the same input, preserving the wording and labeling each excerpt. A sentence can explain a difference the excerpts leave implicit or a trial condition that affects interpretation. End the comparison there. If only one output was captured, quote it and limit the claim to that observation.

For examples to use when refining this skill, see the [golden cases](examples/README.md).
