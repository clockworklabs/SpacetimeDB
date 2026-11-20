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

Write a coverage summary a reviewer can skim. For a test-only change, a useful shape is “Added [kind of test] for [behavior], [behavior], and [boundary].” Use a few short bullets when the behaviors need separate lines. For example: “Added checkout tests for expired vouchers and refunds after cancellation.” Name the regression in product terms. The code diff supplies the permutations, flags, expected values, and assertions.

Include a known failure or consequential gap in its own short sentence. Inspect existing tests before claiming coverage. Distinguish new, updated, and existing coverage when relevant. Routine successful runs, configurations and reruns belong in the author handoff. Include a manual result only when it changes the reviewer's understanding of coverage.

For generated output, lead with what the trial covered. Show the complete comparison inline after the summary. Use a unified diff or two intact outputs for the unit under review. “Before” is the observed output from before the PR. Preserve that baseline. Label a later run on the base revision as a reproduction. Mention trial conditions that affect interpretation. If only one output was captured, show it and limit the claim to that observation.

For examples to use when refining this skill, see the [golden cases](examples/README.md).
