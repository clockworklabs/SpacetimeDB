# Delivery notification application interface

Use `notifications-toggle` to open notifications. Use `notification-item` for each notification
and `notification-unread-count` for the unread total.

The `notifications-panel` has `aria-busy="false"` only when the signed-in account's
notifications have loaded. Keep the panel present when the list is empty.
