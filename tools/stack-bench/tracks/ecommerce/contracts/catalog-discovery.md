# Catalog discovery application interface

New catalog data starts with zero purchases. Do not seed sample orders or purchase counts.
When adding this feature to an existing app, preserve purchases made through the app.

| Element ID | Required element |
| --- | --- |
| `item-list` | Contains exactly the ten ranked storefront items. |
| `item-card` | Shows one storefront or search result. |
| `item-name` | Shows the item name inside its `item-card`. |
| `search-input` | Searches the full catalog as the visitor types or when they press Enter; no separate control runs the search. |
| `search-results` | Contains matching `item-card` results. |
