

## Appendix: Testing Hooks (required)

The app is graded by an automated harness that locates elements **only** via
`data-testid` attributes. Add the exact test IDs below to the corresponding
elements. These are plain HTML attributes — they must not change your design,
styling, architecture, or backend in any way.

Rules:
- Attribute name is exactly `data-testid`; values are exactly as listed (kebab-case).
- Repeated elements (each room in the list, each message) carry the same testid on every instance.
- An element that is hidden until a menu/toggle opens still counts, as long as it is in the DOM after its toggle is clicked.
- Do not add testids beyond this list to elements that could be confused with these.

| Test ID | Element |
|---|---|
| `app-title` | the app's visible title, naming which backend it is built on (for example "PostgreSQL Shop") |
| `item-list` | the container holding the storefront's item cards |
| `item-card` | one per item shown on the storefront; contains that item's name, price and stock |
| `item-name` | the item's name, inside its card |
| `item-price` | the item's price, inside its card |
| `item-stock` | the item's current stock as a number, inside its card |
| `search-input` | the search box, usable signed out |
| `signup-username` | username input on the sign-up form |
| `signup-password` | password input on the sign-up form |
| `signup-submit` | button that submits the sign-up form |
| `signin-toggle` | control that reveals the sign-in form; the sign-up form must remain usable after it is clicked |
| `signin-username` | username input on the sign-in form |
| `signin-password` | password input on the sign-in form |
| `signin-submit` | button that submits the sign-in form |
| `current-user` | the signed-in account's name, shown once signed in |
| `signout` | control that signs the current account out |
| `buy-now` | button on an item card that buys one unit immediately; shown only to signed-in customers |
| `add-to-cart` | button on an item card that adds one unit to the cart; shown only to signed-in customers |
| `cart-toggle` | control that opens the cart |
| `cart-count` | the number of items currently in the cart |
| `orders-toggle` | control that opens the customer's order history |
| `search-results` | the container holding search results; its item cards carry the same hooks as the storefront's |
| `item-detail` | the panel or page showing one item, opened from its card |
| `review-rating` | the form control choosing a rating from 1 to 5 — an input or a select, so it can be driven by automation |
| `review-input` | the text input for a new review's comment |
| `review-submit` | button that submits the review |
| `review-average` | the item's average rating as a number |
| `review-item` | one per review on the item, containing its comment; appears without a reload once submitted |
| `cart-panel` | the cart's container, opened by cart-toggle |
| `cart-item` | one per line in the cart, containing that item's name |
| `cart-quantity` | the quantity of a cart line, as a number |
| `cart-total` | the cart's total price |
| `checkout-submit` | button that turns the cart into an order |
| `order-list` | the container holding the customer's orders, opened by orders-toggle |
| `order-item` | one per order, containing the names of the items bought |
| `order-total` | an order's total price |
| `admin-panel` | the admin area's container; present only for an admin account |
| `admin-item-row` | one per item in the admin item list, containing that item's name |
| `admin-stock` | an item's total stock in the admin item list, as a number |
| `admin-warehouse-item` | one per warehouse in the admin warehouse list, containing its name |
| `admin-location-row` | one per item-in-warehouse holding, containing the item and warehouse it refers to |
| `admin-location-qty` | the quantity held in one warehouse for one item, as a number |
| `restock-input` | the input for how many units to add, inside the item-in-warehouse row it restocks — one per admin-location-row, so the row identifies both the item and the warehouse |
| `restock-submit` | button that applies the restock, inside the same admin-location-row as its restock-input |
| `admin-revenue` | total revenue across all orders, as a number |
| `buy-error` | appears when buying an out-of-stock item, or checking out more than is available |
| `auth-error` | appears on a taken username or a wrong password |
| `review-error` | appears when reviewing an item the customer has never bought; also if a second review is rejected rather than updating the first |
| `out-of-stock` | appears once an item's stock reaches zero, without a reload |
| `empty-cart` | shown before anything is added, and again after checkout empties the cart |
| `cart-remove` | checked with a populated cart by the feature suite |
| `admin-link` | present for an admin account and absent for a customer — both halves are scored |

Before declaring DEPLOY_COMPLETE, verify the hooks by running the contract
linter (command provided in your build instructions) and fix any failures.
