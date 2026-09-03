# Automatic reorder application interface

Use these application interface names:

- `reorder-link` opens the automatic reorder rules for warehouse staff.
- `reorder-item`, `reorder-threshold`, and `reorder-quantity` identify the rule inputs.
- `reorder-submit` saves the rule.
- `reorder-rule-item` identifies each saved rule, sets `data-entity-id` to the rule's item
  identifier, and contains its item name, threshold, quantity, and current state.

Saving a rule is the named `saveReorderRule` application action. For HTTP stacks,
`saveReorderRule` is `PUT /api/reorders/{itemId}` with `threshold` and `quantity` in the body.
For reducer stacks, it is `save_reorder_rule(itemId, threshold, quantity)`.

Use `buy-now` inside an `item-card` to create stock changes that evaluate a reorder rule.
