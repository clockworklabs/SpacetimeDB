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

Summarize coverage at test-title granularity. For newly added tests, count the test functions, introduce them by number and kind alone with a colon, and give each one a plain bullet. Put the motivating regression first. Keep closely related tests adjacent. For existing coverage, identify what was exercised or updated.

Write each main bullet as a test title the caller would recognize. Name the option or operation when it defines the scenario. Use product verbs such as “uses the selected credit”; skip descriptions of how components pass values to one another. If one test also guards other independently breakable behaviors, put those on separate indented lines under its main case. A short parenthesis can group input forms. Preserve reviewer wording when regrouping.

For example, two billing tests might exercise eight assertions. The author also temporarily disabled duplicate detection, saw the first test fail, restored the change, and reran 96 tests. The complete Testing prose could be:

> Added two billing tests covering the following behaviors:
>
> - A duplicate invoice is rejected (CSV or JSON import). Disabling duplicate detection made the test fail.
> - Prepaid invoices use the selected credit. The same test covers:
>   - Credits apply before tax.
>   - Fully covered invoices skip payment.

Stop once the scope is clear. “Requires approval” is the entire permission test title; a second description of normal success or denial adds no new scope. Keep distinct extra coverage, such as *which* approval applies, under its own test. Give a specific error code when that is what the test distinguishes. For a failed mutation experiment, naming the removed protection and saying the test failed is enough. Keep fixture details, build settings, sample sizes and numeric thresholds in the test code unless they define the feature being tested. Omit response text, call ordering, suite status and restoration steps. A manual fault-injection result belongs on the regression bullet. Mention a failure or gap separately when it affects the conclusion.

For generated output, lead with what the trial covered. Show the complete comparison inline after the summary. Use a unified diff or two intact outputs for the unit under review. “Before” is the observed output from before the PR. Preserve that baseline. Label a later run on the base revision as a reproduction. Mention trial conditions that affect interpretation. If only one output was captured, show it and limit the claim to that observation.

For examples to use when refining this skill, see the [golden cases](examples/README.md).
