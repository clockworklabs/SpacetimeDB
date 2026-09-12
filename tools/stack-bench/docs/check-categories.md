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

Categories do not prove that a guarantee was supplied without being asked. That requires the separate [prompt disclosure audit](prompt-boundary-audit.md), including the exact delivered request, contracts, and selected skills. A production requirement can still be explicitly disclosed.

Feature completion counts only fully passed dependency nodes. A node with passing feature checks but unfinished guarantees is not complete. Check completion counts accepted positive-point criteria. Both use the full selected target, including blocked and unmeasured work.

The dashboard's Features/Checks choice changes the counting unit. It is not a
filter for the Feature category. A feature node can contain checks from several
categories. Weighted score is a third measure: passed points divided by selected
points. Keep all three denominators explicit in exported comparisons.
