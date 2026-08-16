## Access control: purchasing

Treat identity as server-enforced authority, not UI decoration. Unauthenticated
callers cannot purchase, one account cannot place an order for another account,
and order history is visible only to its owner. Knowing a username never grants
access to that account.

## Access control: warehouse administration

Customer accounts cannot perform warehouse-administration writes. Enforce this
on the server even if the corresponding controls are hidden in the UI.

## Access control: reviews

Only a customer who purchased an item may review it. Enforce this on the server
instead of relying on whether the review form is visible.

## Access control: cart

One account cannot read or change another account's cart. Refuse a cart request
whose quantity is zero or negative without changing state.

## State durability: accounts

A signed-in session survives a page reload as the same account.

## State durability: account data

The same account keeps its cart and orders across reload and connection loss.
After reconnect it has current state without another sign-in. Restarting the
application must not duplicate starting data or reset state users changed.

## Live state: catalog and purchasing

Stock and best-seller ranking update every affected open storefront without a
reload, including signed-out storefronts.

## Live state: cart

The same account open in two clients sees one current cart; a change in either
client reaches the other without a reload.

## Live state: reviews

Reviews and average ratings update affected open item views without a reload.
A view opened while a review is submitted converges to the current review list.

## Live state: warehouse administration

Restocking updates warehouse quantities, total item stock, and open storefronts
without a reload.

## Concurrency safety: purchasing

Stock never becomes negative and only one customer can receive the last unit.
Do not rate-limit, debounce, or serialize unrelated customers merely to avoid
races.

## Concurrency safety: restocking

Concurrent restocks and purchases preserve every accepted stock change.

## Concurrency safety: checkout

Repeating or racing checkout for the same cart creates only one order.

## Transactional integrity: reviews

A customer has at most one review per item; another submission updates it.

## Transactional integrity: purchasing

The server controls prices and order attribution rather than trusting client
values. Historical order prices do not change.

## Transactional integrity: warehouse accounting

Every stock reduction caused by a sale has its matching order, revenue equals
the orders' recorded totals, and fresh clients agree with those results.

## External data synchronization

Other systems may write stock directly without calling the application. Use
singular tables `item(id, name, price)`, `warehouse(id, name)`, and
`stock(item_id, warehouse_id, quantity)` as the source of truth for this
interoperability surface. Open pages and newly loaded pages must converge to a
direct stock correction, including a correction made while the application
server is down.

