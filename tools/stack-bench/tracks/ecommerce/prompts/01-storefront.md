# Level 1 — Storefront and warehouse

Create a **real-time store**: a public storefront, customer accounts with a cart
and order history, and an admin area for stock across warehouses.

Everyone looking at the store sees the same numbers at the same time. Stock,
prices, ratings and the best-seller ranking are shared state — when one customer
buys the last unit, every other visitor's screen must reflect it without a
reload, whether they are signed in or not.

## UI & Style Guide

### Layout
- **Header**: store name, search box, cart button with item count, account area
  (sign in / current user), admin link for admin accounts only
- **Storefront** (main): grid or list of item cards
- **Item detail**: name, price, stock, description, reviews
- **Panels** (slide-in or overlay): cart, order history, admin

### Visual Design
- Dark theme using the brand colors from the language section below
- Background: darkest shade for main bg, slightly lighter for cards and panels
- Text: light on dark, muted color for secondary info (SKU, timestamps)
- Borders: subtle 1px, low contrast against background
- Consistent spacing scale (8/12/16/24px)
- Font: system font stack, clear hierarchy (bold headers, regular body, small muted metadata)
- Rounded corners on inputs, buttons, cards
- Prices right-aligned and consistently formatted

### Components
- **Item card**: name, price, stock state, and — for signed-in customers — buy and
  add-to-cart actions. Out-of-stock cards are visibly distinct and their buy
  action is disabled.
- **Inputs**: full-width, rounded, subtle border, placeholder text, focus ring using primary color
- **Buttons**: filled with primary color for main actions, outlined/ghost for secondary
- **Badges**: small pill-shaped with count (cart count, "Out of stock")
- **Status indicators**: stock level shown as a number, low stock visually distinct
- **Modals/panels**: slide-in from right with subtle backdrop

### Interaction & UX
- Show loading/connecting state while the backend connects (spinner or skeleton, not blank screen)
- Empty states: helpful text for an empty cart, no orders, no search results, no reviews
- Error feedback: inline error messages or toast notifications, never silent failures
- Smooth transitions: fade/slide for panels and modals
- Keyboard support: Enter submits forms, Escape closes modals/panels

## Features

### Accounts

- A visitor can **create an account** with a username and password
- A returning visitor can **sign in** with those credentials
- A signed-in user can **sign out**, returning to the signed-out state
- A signed-in session **persists across a page reload** — a returning user is not asked
  to sign in again — and across the connection dropping and re-establishing. The same
  person stays the same account throughout, keeping their cart, orders and reviews.
- Usernames are unique. Signing up with a taken username fails with a visible error and
  must never sign the visitor in as the existing account.
- Signing in with a wrong password fails with a visible error.

Identity is an account, not a display name: knowing someone's username must never
grant access to it.

### Browsing (signed out or signed in)

- The storefront shows the **10 most-purchased items**, most purchased first.
  Items with the same number of purchases are ordered **alphabetically by name**.
- Every item shows its **name, price and current stock**
- The ranking and the stock numbers are **live**: when anyone buys something, every
  visitor's storefront updates without a reload — including visitors who are not
  signed in
- **Search** finds items by name, matching any part of the name, case-insensitively.
  Search covers the whole catalogue, not just the items currently on the storefront.
- Anyone can open an item and read its **reviews** and average rating

### Buying

- **Only signed-in customers can buy.** A signed-out visitor sees no buy or
  add-to-cart action, and the server must refuse a purchase request that arrives
  without a valid session — not merely hide the button.
- Buying an item **reduces its stock by one for everyone**, immediately
- A purchase is a **sale, not just a stock movement**: it creates an order for the buyer,
  visible in their order history, and its price counts towards total revenue — exactly as a
  checkout does. Stock must never leave a warehouse without a corresponding order.
- An item with **zero stock cannot be bought**. The attempt fails with a visible error
  and changes nothing.
- **Stock may never go negative, and two customers must never both get the last unit.**
  Many customers may be buying the same item at the same instant.
- Purchases are **not rate limited** — a customer may buy repeatedly, and many
  customers may buy simultaneously. Do not throttle, debounce or queue purchases.

### Cart

- A signed-in customer can **add an item to their cart**, change the quantity, and
  remove it
- The cart belongs to the **account, not the browser**: the same account signed in
  twice sees one cart, and a change made in one place appears in the other without a
  reload
- The cart **survives a reload and a sign-out/sign-in**
- Adding an item that is already in the cart **raises its quantity** rather than
  adding a second line for it
- **Checkout** turns the cart into one order, reduces stock for every line, and
  empties the cart. Checking out twice must not produce two orders for the same cart.
- Checkout fails, changes nothing and explains why if any line exceeds available stock

### Order history

- A cart belongs to one account and **nobody else can read or change it**
- A cart line's quantity is **at least one**. A request carrying zero or a negative
  quantity is refused and changes nothing — it must never reduce an order's total.
- A customer can see **their own past orders**, newest first, each showing its items,
  quantities, the price paid and an order total
- The price recorded on an order is **the price at the time of purchase** — later price
  changes do not alter past orders
- A customer sees only their own orders

### Reviews

- A customer can **write a review** of an item **they have bought**. The review form is
  shown to every signed-in customer, but the **server** refuses a review from someone who
  has never ordered the item, with a visible error — a rating is a claim about a purchase,
  and hiding the form is not enforcement.
- **One review per customer per item.** A second attempt updates their existing review
  rather than adding another.
- Reviews are visible to everyone, including signed-out visitors
- Each item shows its **average rating**, which updates live as reviews arrive

### Admin

- Admin accounts see an **admin area**; customers do not, and the server must refuse
  admin actions from a non-admin account
- Admin lists **every item** with its stock, **every warehouse**, and the **stock of
  each item in each warehouse**. These are live like every other view: a purchase or a
  restock moves the admin's numbers without a reload.
- An admin can **restock**: add units of an item to a named warehouse
- An item's stock on the storefront is the **sum of that item's units across all
  warehouses**. A restock in one warehouse raises the storefront number live, for
  everyone.
- Admin sees **total revenue**: the sum of all order totals, which must always equal
  what the orders themselves add up to

### Starting data

The store ships with a fixed catalogue so it is never empty. On startup, if there
are no items yet, create exactly this data (and nothing else):

**Warehouses:** `East`, `West`

**Items** — stock is split across the two warehouses as shown, so an item's
storefront stock is the sum of its two rows:

| Item | Price | East | West |
|---|---|---|---|
| Air Purifier | 189.00 | 60 | 40 |
| Bluetooth Speaker | 79.50 | 50 | 50 |
| Coffee Grinder | 64.00 | 70 | 30 |
| Desk Lamp | 42.00 | 55 | 45 |
| Espresso Machine | 449.00 | 80 | 20 |
| Gaming Mouse | 59.00 | 50 | 50 |
| Headphones | 199.00 | 60 | 40 |
| Induction Cooktop | 329.00 | 50 | 50 |
| Keyboard | 89.00 | 70 | 30 |
| Laptop Stand | 29.00 | 90 | 10 |
| Mirrorless Camera | 1299.00 | 2 | 1 |
| Webcam | 69.00 | 60 | 40 |

**Admin account:** username `admin`, password `stackbench-admin-2026`. It is an
admin; every account created through sign-up is a customer.

No purchases, orders or reviews exist at the start — so every item begins with the
same purchase count and the storefront's opening order is alphabetical, which means
`Mirrorless Camera` and `Webcam` are the two items not on the front page until
something is bought.

Do not seed this data again if it is already there: restarting the server must not
duplicate the catalogue or reset stock that customers have changed.
