# Address-book migration: expected production behavior

Draft specification, not a registered pack or qualified scored selection.
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
Scored IDs, points and recipe registration wait for executable observations and
matching correct/defective reference qualification. Historical scores stay fixed.
