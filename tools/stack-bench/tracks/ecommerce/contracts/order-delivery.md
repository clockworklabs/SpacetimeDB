# Order delivery application interface

Use `completed-order-item` for each completed order in the staff view. Use
`completed-order-status` inside it for the current state. This extends the order lifecycle:
after `shipped`, `order-status` and `completed-order-status` read `delivered`.
