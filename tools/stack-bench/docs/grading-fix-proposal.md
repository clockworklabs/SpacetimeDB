# Stack Bench ecommerce fix proposal

Status: proposed changes. Stock-alert privacy was implemented in 9a1685d8dd; the other changes are not implemented.

**Fix the demonstrated false passes first: stock-alert privacy and catalog coverage.** Preserve comparison conditions in the draft publisher. Treat form-encoded authentication as a bounded extension of measurement support.

Guarantees remain required. A failed guarantee continues to block dependent work under the current progression policy. These fixes strengthen the evidence used by that policy. They do not change feature weights, add scoring points, or excuse failed guarantees.

| Change | Readiness | Main unresolved detail |
| --- | --- | --- |
| Stock-alert privacy | Implemented in 9a1685d8dd | None |
| Catalog coverage | Check change is clear | Confirm fixture and reset behavior for every caller of the shared scenario |
| Upload conditions | Export omission is reproduced; producer change is clear | Inspect and coordinate the receiving schema and grouping logic |
| Form-encoded authentication | Support gap is confirmed | Prove correct request modification through both existing capture paths |

## 1 Detect private notifications delivered to the wrong customer

The [stock-alert privacy criterion](../tracks/ecommerce/scenarios/progression-stock-alerts.json) checks that the owner sees an alert and another customer does not display it. A local probe passed when the server sent the private notification to both customers and the page hid it from the second customer. This is a false pass for server privacy.

### Proposed change

Extend the existing privacy criterion to check actual delivery:

1. Create a stock alert for customer A through the normal application flow.
2. Confirm that A receives and displays the resulting notification.
3. Start customer B in a fresh browser session. Begin traffic capture before login and notification loading, so early delivery is included.
4. Confirm that B successfully loads the notification view. A failed read must not look like a successful empty result.
5. Check that B neither receives A's notification nor displays it during the defined observation window.

The positive owner check remains essential. An application that suppresses all notifications must not pass privacy by doing nothing.

### Detail that must be proved before implementation is complete

The current [interface](../tracks/ecommerce/contracts/stock-alerts.md) exposes the product name in the alert. The product name is public. Searching all network traffic for “Air Purifier” would wrongly classify ordinary catalog data as a leak.

The existing [transport assertions](../src/actions/actor-transport-action-executors.ts) support checking for received text. They do not, by themselves, identify a private notification record. Reusing them with a public name is not a fix.

First determine whether the real notification can be identified from existing captured responses or subscription data. If that is insufficient, define the smallest additional interface needed to relate the owner's displayed alert to its real backend record. A record reference is a candidate, not a settled design.

The match must include enough context to distinguish a notification from unrelated records with the same numeric ID. It must also account for the supported transport's actual representation. A frontend-only marker, a separate diagnostic endpoint, or an application-supplied claim that access was refused would not prove privacy on the real path. A leaked notification that omits a proposed marker is a case the proof must address.

Use the existing capture and decoding facilities where they can establish this relationship. If they cannot establish it for a selected stack, retain an inconclusive outcome for that scope. Do not convert missing observation into a pass.

### Acceptance cases

- The owner receives the alert and the other customer receives a successful empty notification result: pass.
- The server leaks the alert and the page displays it: fail.
- The server leaks the alert but the page filters it out: fail.
- The leaked record holds only internal ids, such as an account id and an item id, and no product name: fail. A real SpacetimeDB app on the leaderboard stores alerts this way in a public table.
- Both customers receive ordinary public catalog data: no privacy failure from the public product name.
- The owner receives no alert: cannot pass the criterion.
- Capture is incomplete or cannot identify the relevant delivery: inconclusive.

Keep the same criterion and points. Add the client-filtered leak to the deliberately defective controls. Then validate the criterion through the normal database and restock flow on each stack in the affected qualification scope. The earlier probe replaced those setup steps with prepared data; it is evidence of the assertion gap, not full qualification of the repair.

## 2 Verify every required catalog item

The [catalog criterion](../tracks/ecommerce/scenarios/01-catalog-values.json) checks only Air Purifier and its values. It passed a page with that one item while the other 12 fixture items were absent.

### Proposed change

Extend the existing criterion to verify each required item's exact name, price, and total stock. Use the fixture selected by the compiled recipe as the authority for expected values. Do not introduce a second independently maintained catalog.

Match each required item by its identity and verify the values within that item's card. A total card count is insufficient: a duplicate can replace a missing product without changing the count. Use the existing [catalog controls](../tracks/ecommerce/contracts/catalog-items.md) and browser assertions where possible. The test should not require a particular item order or visual layout.

Before expanding the assertions, trace all recipes that use this scenario. Confirm that each invocation observes the intended starting state. In a flow that retains earlier changes, initial stock or price may no longer be the correct expectation. Do not silently reset retained application data to make the check pass. Resolve the expected state through the existing scenario lifecycle.

Keep this as the existing scored criterion. Checking more of the same requirement does not create more features or points.

This criterion belongs to a level 1 feature that nearly all later work depends on. Under the current gate policy, one wrong value on any of the 13 items would block almost the whole benchmark for that app. Before adopting the wider check, regrade level 1 of the saved apps on every stack and count how many results change.

### Acceptance cases

- Every required item has the correct values: pass.
- Air Purifier is correct but another required item is missing: fail.
- A duplicate item replaces the missing item: fail.
- A different item's price or stock is wrong: fail.
- Required items appear in a different order: pass.

Add a defective control that removes an item the old check did not inspect. Retain the existing wrong-price control. Validate all affected fixture and recipe combinations, including any caller with retained state.

## 3 Preserve experimental conditions in uploads

The current draft [publisher](../src/campaigns/campaign-publish.ts) drops condition information that the retained [report](../src/campaigns/campaign-report.ts) uses to separate comparisons. A local test with two different prompt conditions confirmed the omission. The receiving website has not been inspected, so incorrect website grouping is not an established finding.

### Proposed change

Carry the condition ID and hash for each attempt. Preserve the existing comparison identity and enough approved condition metadata to explain it. Use the retained campaign and the report's current comparison logic; do not invent a separate grouping rule for uploads.

The representation must preserve relevant distinctions such as guidance, specification disclosure, work selection, repair policy, provider settings, and output limits. Reuse shared metadata once at campaign or condition level where appropriate, with explicit references from attempts. A hash identifies a condition but cannot explain its contents without that metadata or an accessible manifest.

Inspect the receiving contract before changing the payload. Version the contract if required. Verify that the receiver stores the distinctions and uses them when grouping comparisons. Do not export credentials or unrestricted private campaign content. Existing uploads without condition data must remain identifiable as incomplete for that comparison purpose; do not invent their missing settings.

### Acceptance cases

- Two attempts with the same stack and model but different prompt conditions retain distinct identities and meaningful condition descriptions after export and import.
- The receiver preserves the same comparison distinctions as the retained report.
- Missing or unknown condition references cannot silently merge into a normal comparison group.
- Existing exclusion, qualification, and cost-status information remains intact.

The producer change is ready to specify. End-to-end completion depends on inspecting and testing the receiver, not just adding fields to the JSON.

## 4 Measure ordinary form-encoded authentication

The [account contract](../tracks/ecommerce/contracts/accounts.md) does not require JSON authentication requests. The generic [credential probe](../src/actions/auth-request-patch.ts) cannot measure certain form-encoded submissions and returns inconclusive. This is a support limit, not proof of an application defect.

### Proposed change

Add explicit support for `application/x-www-form-urlencoded` in the existing credential probe. Parse the form, locate the intended credential field, apply the requested change, and send it in the same format.

Preserve the destination, method, cookies, security tokens, and unrelated form fields. Correctly encode special characters so the test value remains a value rather than creating additional fields. Do not guess when duplicate field names or multiple requests make the target ambiguous.

Both the generic credential patch and the selected signup-write path inspect request bodies. Supporting the format in only one path would leave the other unable to measure it. Trace both callers and preserve their request sequencing and completion evidence. Keep existing JSON and native-stack handling intact.

Form encoding does not by itself settle redirect handling. Follow the current delivery rules unless a separate bounded proof establishes a safe change. Multipart forms, arbitrary binary protocols, and ambiguous multi-request flows are outside this proposed extension.

### Acceptance cases

- An ordinary form request reaches the actual server with only the intended field changed.
- A secure implementation refuses the attempted bypass and leaves protected behavior inaccessible.
- A deliberately vulnerable implementation accepts the bypass and is detected by the application assertions.
- Special characters survive encoding correctly; unrelated fields and credentials remain intact.
- An ambiguous target or uncertain delivery remains inconclusive.
- Existing JSON and native-stack cases still produce the correct outcomes.

Extend the existing [authentication tests](../tests/auth-request-patch.test.ts), then verify the affected ecommerce scenario through its normal action path. A successful parser test alone is insufficient.

## Implementation order and qualification

Start with the small privacy measurement proof because it addresses the most serious false pass. The catalog correction can proceed independently. Keep the publisher and authentication work as separate changes with their own validation.

For changes to qualified checks, contracts, controls, or runtime behavior:

- Identify affected checks through the existing dependency and evidence identities.
- Mark affected definitions as draft and preserve the original qualification records.
- Run the relevant correct and deliberately broken cases. Retain evidence of failures as well as passes.
- Reuse unaffected evidence only through the existing validated reuse process.
- Restore qualification only after evidence matches the changed behavior and required scope.

This proposal does not require a blanket rerun of unchanged checks. It also does not authorize paid campaigns or long qualification runs. Exact run scope must be specified before those actions.

## Evidence and completion status

The [design review](design-review.md#appendix-implementation-findings-and-evidence-limits) records the reproduced findings and their limits. The scripts and receipts are in [design-review-20261014](../qualification-evidence/design-review-20261014/).

The proposal is complete when the changes, acceptance cases, and open measurement questions are clear. A fix is complete only when its normal application path and required qualification have passed. No fix is claimed complete here.
