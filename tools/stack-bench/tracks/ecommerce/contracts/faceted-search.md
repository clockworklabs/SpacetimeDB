# Faceted search application interface

| Element ID | Required element |
| --- | --- |
| `category-filter` | Sets the category filter; a text input, or a `select` whose option values are the category names. |
| `minimum-price` | Sets the inclusive minimum price. |
| `maximum-price` | Sets the inclusive maximum price. |
| `in-stock-filter` | Toggles the in-stock-only filter, which starts off. |
| `search-results` | Contains the filtered page; with no search text and no filter selected, the current page of the full catalog. |
| `item-card` | Shows one result inside `search-results`. |
| `search-next-page` | Opens the next page. |
| `search-previous-page` | Opens the previous page. |

If a `filter-apply` control exists, activating it applies the filters; otherwise results update
as each filter changes.
