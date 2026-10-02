# Level 3 production behavior

## Durable reservations

Pending reservations survive a backend restart.

## Durable restocks

Pending restocks survive a backend restart.

## Durable order delivery

Pending order delivery survives a backend restart.

## Durable cart expiration

Pending cart expiration survives a backend restart.

## Exactly-once restocks

Restarting the backend cannot apply a restock more than once.

## Exactly-once delivery

Restarting the backend cannot apply an order transition more than once.

## Server-timed restocks

A pending restock does not run early after a restart.

## Server-timed reservations

A reservation expires without an open browser.

## Deferred-work access

Only an admin can schedule or cancel a restock.

## Stock conservation

Reservation expiry returns exactly the stock that the reservation took. Checkout does not take reserved stock twice.
