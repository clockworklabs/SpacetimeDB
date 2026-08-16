## Public testing interface

The runner locates visible controls through exact `data-testid` attributes.
These attributes are only an observation interface; they do not prescribe UI
structure, data modeling, libraries, transport, or implementation strategy.

## Catalog hooks

| Test ID | Observable element |
|---|---|
| `app-title` | visible application title naming the selected backend |
| `item-list` | storefront item-card container |
| `item-card` | one storefront or search-result item |
| `item-name` | item name within a card |
| `item-price` | numeric item price within a card |
| `item-stock` | numeric total stock within a card |
| `search-input` | catalog search input |
| `search-results` | search-result item-card container |
| `item-detail` | opened item detail view |
| `out-of-stock` | visible out-of-stock state |

## Account hooks

| Test ID | Observable element |
|---|---|
| `signup-username` | sign-up username input |
| `signup-password` | sign-up password input |
| `signup-submit` | sign-up submit control |
| `signin-toggle` | control that reveals sign-in |
| `signin-username` | sign-in username input |
| `signin-password` | sign-in password input |
| `signin-submit` | sign-in submit control |
| `current-user` | active account name |
| `signout` | sign-out control |
| `auth-error` | visible account error |

## Purchasing hooks

| Test ID | Observable element |
|---|---|
| `buy-now` | item-card control that buys one unit |
| `orders-toggle` | control that opens order history |
| `order-list` | order-history container |
| `order-item` | one order containing its item names |
| `order-total` | numeric order total |
| `buy-error` | visible purchase or checkout error |

## Cart hooks

| Test ID | Observable element |
|---|---|
| `add-to-cart` | item-card control that adds one unit |
| `cart-toggle` | control that opens the cart |
| `cart-count` | numeric count of units in the cart |
| `cart-panel` | cart container |
| `cart-item` | one cart line containing its item name |
| `cart-quantity` | numeric quantity for a cart line |
| `cart-total` | numeric cart total |
| `cart-remove` | control that removes a cart line |
| `checkout-submit` | checkout control |
| `empty-cart` | visible empty-cart state |

## Review hooks

| Test ID | Observable element |
|---|---|
| `review-rating` | rating input or select with values one through five |
| `review-input` | review comment input |
| `review-submit` | review submit control |
| `review-average` | numeric average rating |
| `review-item` | one visible review containing its comment |
| `review-error` | visible review error |

## Warehouse administration hooks

| Test ID | Observable element |
|---|---|
| `admin-link` | control that opens warehouse administration |
| `admin-panel` | warehouse-administration container |
| `admin-item-row` | one item row containing its name |
| `admin-stock` | numeric total stock within an item row |
| `admin-warehouse-item` | one warehouse entry containing its name |
| `admin-location-row` | one item-and-warehouse holding row |
| `admin-location-qty` | numeric quantity within a holding row |
| `restock-input` | quantity input within a holding row |
| `restock-submit` | restock control within the same holding row |
| `admin-revenue` | numeric total revenue |

