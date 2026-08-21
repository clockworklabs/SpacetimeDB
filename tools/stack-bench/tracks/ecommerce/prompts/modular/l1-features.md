## Accounts

Visitors can create an account with a username and password. Returning users
can sign in with those credentials, see which account is active, and sign out.
Show a useful error for a taken username or incorrect password.

Use these ordinary server write names so the test runner can exercise the same
operations as the UI: `POST /api/auth/signup` and `POST /api/auth/signin` on
server-based stacks, or reducers `signUp` and `signIn` on SpacetimeDB.

## Catalog

Show the ten most-purchased items, with alphabetical ordering as the tie-break.
Each card shows name, price, and total stock. Search matches any part of an item
name, case-insensitively, across the full catalog. An item detail view shows its
description, reviews, and average rating.

## Purchasing and orders

A signed-in customer can buy one unit of an item. A purchase reduces stock,
creates an order for that customer at the current price, and appears in their
order history. Order history shows newest orders first with items, quantities,
prices paid, and totals. Show a useful error when a purchase cannot be completed.
Do not start with purchases or orders.

Name the ordinary buy operation `POST /api/items/:id/buy` on server-based stacks
or reducer `buyNow` on SpacetimeDB.

## Cart and checkout

A signed-in customer can add an item to a cart, change its quantity, remove it,
and see the cart total. Adding the same item again raises the existing line's
quantity. Checkout creates one order from the cart, reduces stock for its lines,
and empties the cart. Explain why a checkout cannot be completed.

Name the ordinary writes `POST /api/cart` and `POST /api/checkout` on
server-based stacks, or reducers `addToCart` and `checkout` on SpacetimeDB.

## Reviews

A signed-in customer can rate an item from one to five and write a comment.
Show reviews and the average rating on the item detail view, including to
signed-out visitors. Show a useful error when a review cannot be submitted.
Do not start with reviews.

## Warehouse administration

Provide an administration area that lists every item, both warehouses, and the
quantity of each item held at each warehouse. It can add units to a selected
item and warehouse and shows total revenue across orders.

Create its administrator account with username `admin` and password
`stackbench-admin-2026`.

Name the ordinary restock operation `POST /api/admin/restock` on server-based
stacks or reducer `adminRestock` on SpacetimeDB.
