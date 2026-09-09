# Ecommerce prompt boundary audit

The neutral condition measures production guarantees supplied without explicit requests.
It must not be designed to make a chosen stack fail. All stacks receive the same product
work and equivalent application interfaces. Stack setup instructions remain stack-specific.

## What changed

Reviewed the selected modular feature requests and contracts through all six dependency
depths, plus sequential L1-L3 framing and action fragments. The exact rendered dependency
request is checked for all three stacks at every depth.

| Source | Removed from agent-facing product work | Retained |
|---|---|---|
| Stock transfers | Atomic quantity changes and conservation instruction | Move stock between named warehouses |
| Cancellation and returns | Explicit revenue reconciliation instructions | Cancel before shipping, return after shipping, refund/restock policy and visible order state |
| Payments and refunds | Retry deduplication and exactly-once instructions | Payment/refund display; successful refund resolves support case |
| Price changes and delivery | No-reload instructions and cancelled-order progression rule | Price editing, delivered state after 60 seconds |
| Support and recommendations | Cross-account isolation instructions | Product actions and recommendation ranking |
| Automatic reorder | Pending-work deduplication instruction | Threshold and restock inputs |
| Interface contracts | Repeated authorization, conservation, price and live-update instructions | Hooks, routes/reducers, identifiers, value formats, navigation and readiness |
| Sequential cart input | The adversarial quantity and explicit rejection instruction | Item identifier; scenario supplies its own quantity |
| Sequential framing | General real-time/restart/production guarantees | Current product scope and starting data |

Expected specification documents remain unchanged. They are still available when a study
explicitly selects requested specifications. They do not enter neutral dependency requests.
No check, scenario, point value, or pass threshold was weakened by these prompt edits.

## Necessary boundaries

A product request still states what the product does: buyer reviews, cancellation before shipping,
reservations, stock scheduling, and account-related features. These semantics must be clear.
A statement such as "a shipped order becomes delivered after 60 seconds" defines the feature;
restart survival, duplicate execution, clock authority, and cross-account access do not need
implementation instructions in that request. Refund and restock policy remains explicit: a
return could otherwise reasonably enter quarantine rather than saleable inventory. Verified-buyer
reviews are a product policy, not a universal production guarantee. Keep that policy in the
request and measure its enforcement separately.

An interface still needs deterministic names and formats. A readiness flag must distinguish
an empty result from a failed read. An action must be the actual UI action, not a separate
endpoint that can pass while the product is broken. Direct stock-table access remains because
external-write scenarios need a stable integration surface. These hooks reveal an operation's
existence, but must not state the expected security or synchronization policy.

The cart quantity action reads its item identifier from the app. Its private scenario retains
the invalid quantity in the named action arguments. The shared executor already fills omitted
fields from those arguments; no new runtime path is needed.

## Skills and interpretation

The former neutral profile included full server and client skills. They contain useful
production design advice, including session preservation and identity ownership. That was
inconsistent with this study's goal, even though specification packs were not requested.
Keep normal development skills intact. Neutral material selection must exclude design advice;
CLI and selected dev-workflow instructions can remain as execution tooling. Pin that changed
material as a new experimental condition. Historical frozen runs keep their original material.

Repairs intentionally reveal observed defects. A run with repairs measures assisted recovery,
not production guarantees supplied without feedback. Use first-build results or a no-repair
condition for the primary unprompted-guarantee analysis.

## Reporting and next validation

Separate UI, feature, and production-quality checks. Report L3-only results alongside cumulative
results. A high cumulative percentage must not obscure an authorization or concurrency failure.
Some older feature packs contain production-quality checks, so category cannot be inferred from
pack type or scoring treatment. Classification and disclosure are separate axes.

All affected definition evidence must be renewed. Do not relabel historical results as if they
used these requests. Validate the same checks on reference apps before publishing a verified
comparison. The next experiment must compare stacks under the same pinned prompts, guidance,
repair policy and budgets; it cannot assume which stack will fail.
