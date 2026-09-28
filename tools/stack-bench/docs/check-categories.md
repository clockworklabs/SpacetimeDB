# Check categories

A category describes the property a criterion tests. It does not describe how the grader reaches the app.

- **Feature:** a requested product operation or result, such as finding an item or creating a ticket.
- **Production:** an access boundary, ownership rule, consistency property, durability, concurrent correctness, deduplication, or synchronization property.
- **Interface:** presentation or a test hook without a separate product or production assertion. The reservation countdown is the current example.
- **Unknown:** the frozen definition does not contain category metadata. Old results keep this label. Current source must not classify historical results retroactively.

Read counts from the frozen campaign's selected criteria, not from every criterion
in a scenario file. A depth-limited campaign selects only part of the graph.
Zero-point setup controls remain evidence but do not enter scored check completion.
The [compiled recipe](../tracks/ecommerce/composition/README.md#authoring-commands)
owns category and point metadata; this document does not maintain a second count.

`category` is authored on the scenario criterion and copied into the compiled recipe and feature catalog. `role` stays separate. Roles control progression and prerequisites; categories only split reports. Neither point weights nor dependency gates change.

A mixed criterion is production when passing it requires a production property. For example, a refund check that also tests duplicate refusal is production. This coarse label does not separate which assertion failed. Split such a criterion only through a reviewed definition change with new qualification evidence.

Read the failed step and structured finding before assigning a cause. Check
details identify a missing or invalid named-action or stock interface and state
that later steps were not reached. A recorded failure still earns no credit.
It does not establish that a later ownership, conservation, or durability
assertion failed. Several checks can stop at the same missing interface; do not
count them as independent application defects. Other control failures still
need review: a missing error marker does not prove that a refused write occurred.

Password security remains a required account guarantee. Normal account creation,
duplicate-account refusal, and sign-out/sign-in control access to dependent
features. A failed password guarantee prevents full account completion, but does
not stop otherwise usable accounts from opening later work. This policy applies
to newly compiled dependency campaigns. It does not change frozen campaigns or
provide scores for features that an earlier agent was never asked to build.

Categories do not prove that a guarantee was supplied without being asked. That requires an audit of the exact delivered request, contracts, and selected skills against the [request boundaries](prompting.md#request-boundaries). A production requirement can still be explicitly disclosed.

Feature completion counts only fully passed dependency nodes. A node with passing feature checks but unfinished guarantees is not complete. Check completion counts accepted positive-point criteria. Both use the full selected target, including blocked and unmeasured work.

The dashboard's Features/Checks choice changes the counting unit. It is not a
filter for the Feature category. A feature node can contain checks from several
categories. Weighted score is a third measure: passed points divided by selected
points. Keep all three denominators explicit in exported comparisons.
