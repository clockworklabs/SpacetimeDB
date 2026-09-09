# Check categories

A category describes the property a criterion tests. It does not describe how the grader reaches the app.

- **Feature:** a requested product operation or result, such as finding an item or creating a ticket.
- **Production:** an access boundary, ownership rule, consistency property, durability, concurrent correctness, deduplication, or synchronization property.
- **Interface:** presentation or a test hook without a separate product or production assertion. The reservation countdown is the current example.
- **Unknown:** the frozen definition does not contain category metadata. Old results keep this label. Current source must not classify historical results retroactively.

The current graph sources contain 157 criteria: 103 production, 53 feature, and 1 interface. One feature criterion is a zero-point restock precondition. The scored graph therefore contains 156 checks: 103 production, 52 feature, and 1 interface. A depth-limited campaign selects a smaller scope. These counts describe this definition, not past campaigns.

`category` is authored on the scenario criterion and copied into the compiled recipe and feature catalog. `role` stays separate. Roles control progression and prerequisites; categories only split reports. Neither point weights nor dependency gates change.

A mixed criterion is production when passing it requires a production property. For example, a refund check that also tests duplicate refusal is production. This coarse label does not separate which assertion failed. Split such a criterion only through a reviewed definition change with new qualification evidence.

Categories do not prove that a guarantee was supplied without being asked. That requires the separate [prompt disclosure audit](prompt-boundary-audit.md), including the exact delivered request, contracts, and selected skills. A production requirement can still be explicitly disclosed.

Feature completion counts only fully passed dependency nodes. A node with passing feature checks but unfinished guarantees is not complete. Check completion counts accepted positive-point criteria. Both use the full selected target, including blocked and unmeasured work.
