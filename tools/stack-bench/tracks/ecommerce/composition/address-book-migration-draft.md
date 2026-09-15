# Address-book migration: expected production behavior

Draft recipe for nonbillable development, not a qualified scored selection.
This is an optional existing-app upgrade task. It is not part of the core
storefront tests or their scores. Its qualification is not required to run or
qualify the core benchmark. Shared runner changes still require regression
checks; choosing the migration recipe is what enables populated-state behavior.
The normal product request and interface live under `prompts/modular` and
`contracts`. This document is not delivered to the coding agent.

| Requirement | Independent observation | Required defect control |
| --- | --- | --- |
| Import each nonempty saved profile once, with exact text and owner | Trusted pre-change profiles versus each owner's fresh, complete address-book read | Missing import, changed text, wrong owner, duplicate import |
| Preserve the selected existing business records | Before/after stored account, catalog, stock, order and payment records | Lost order, changed historical price, altered stock, missing payment, changed role |
| Keep IDs and user choices after normal startup | Fresh reads before/after restart following edits and default selection | Regenerated IDs, reverted edit/default, repeated import |
| Keep deletion durable | Delete the imported entry, then restart and read again | Legacy address resurrected; empty book reimported |
| Keep legacy profile behavior coherent | Read and write the old profile after default changes and an empty book | Stale profile; write changes a nondefault entry |
| Reject cross-account access | Actual unauthenticated/other-account read and write requests against a known entry; owner rereads afterward | UI-only access restriction; unauthorized mutation or disclosure |
| Keep the existing store usable | Fresh authorized purchase and stored order/stock effects after migration | Read-only migration, broken checkout, changed permissions |

The initial pure comparators cover only profile import and the existing
checkout reader's selected records. They do not establish reader authenticity,
lease identity, full database coverage, runtime migration success or authorization.
Missing or malformed observations remain measurement errors. Qualified readers
must distinguish a measured missing record from a failed read.

Nonempty means either the saved name or address is nonempty. Preserve all text
exactly, including whitespace and Unicode. Empty profiles create no entry.
Equal address text from different customers does not merge their ownership.
Physical rows, tables, embedded documents, eager conversion and lazy conversion
are valid alternatives. Check observable behavior, not a chosen storage layout.

Stable IDs are tested after the first imported observation; the grader does not
invent an expected ID. Default-delete behavior follows the product request.
The current observations form one selectable workflow because its steps share
saved IDs and application state. The preparation's stored checkout observations
are bound to the original checkpoint. They are compared before and after address
edits; a new checkout must then produce the expected order, payment and stock
effects. These readers cover selected business records, not every account field
or role. Promotion requires matching main-runner correct/defective qualification.
Historical scores stay fixed.
