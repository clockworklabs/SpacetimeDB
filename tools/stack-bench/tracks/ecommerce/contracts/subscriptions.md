# Subscription interface

Open with `subscriptions-link`. Use `subscription-item-input` (item name), `subscription-quantity-input`, `subscription-interval-input` (whole seconds, minimum 30), `subscription-deliveries-input` (1–12), and `subscription-create`.

The loaded `subscriptions-panel` has `aria-busy="false"`. Each `subscription-row` includes the item name and `subscription-status`: `active`, `paused`, `cancelled`, or `complete`. Use `subscription-pause`, `subscription-resume`, and `subscription-cancel`; each exposes `data-action-input` with `subscriptionId`. Each processed slot has one `subscription-delivery` inside the row, with `subscription-delivery-status` (`paid` or `skipped`). `subscription-total` shows the sum of its recorded payments in major currency units. Ordinary orders and payment records include their item names.

<!-- interface:http -->
`pauseSubscription` is `POST /api/subscriptions/{subscriptionId}/pause`.
`resumeSubscription` is `POST /api/subscriptions/{subscriptionId}/resume`.
`cancelSubscription` is `POST /api/subscriptions/{subscriptionId}/cancel`.
<!-- /interface -->

<!-- interface:reducer -->
`pauseSubscription` is `pause_subscription(subscriptionId)`.
`resumeSubscription` is `resume_subscription(subscriptionId)`.
`cancelSubscription` is `cancel_subscription(subscriptionId)`.
<!-- /interface -->
