# Catalog item application interface

The public catalog may be on the first page or a separate page. If navigation is
needed, show `catalog-link` directly on the signed-out first page to open the catalog.

| Element ID | Required element |
| --- | --- |
| `catalog-link` | Opens the public catalog; may be omitted when the catalog is already visible. |
| `item-list` | Contains the public catalog items. |
| `item-card` | Shows one catalog item. |
| `item-name` | Shows exactly the item name inside its `item-card`; activating it opens `item-detail`. Put icons, badges, and other text outside this element. |
| `item-price` | Shows the numeric item price inside its `item-card`. |
| `item-stock` | Shows total stock inside its `item-card`. |
| `item-detail` | Contains the selected item's details. |
