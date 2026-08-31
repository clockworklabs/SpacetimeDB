# Level 3 product work

## Reservations

- Adding an item to a cart reserves its stock immediately for 90 seconds.
- The cart shows the remaining reservation time.
- Checkout converts a live reservation into a sale.
- An expired reservation remains visible as expired.
- Adding the item again renews the reservation.

## Scheduled restocks

- An admin can schedule and cancel a restock.
- A pending restock shows its remaining time.
- A due restock updates stock, leaves the pending list, and enters the stock ledger.

## Order delivery

- A shipped order becomes delivered 60 seconds after shipping.
- Delivery appears live in the customer order history and the staff view.
- A cancelled order never advances.

## Cart expiration

- A cart with no activity for five minutes expires and releases its reservations.
- A returning customer sees the expired notice and an empty cart.
