# Progression feature testing interface

## Faceted search hooks

Use `category-filter`, `minimum-price`, `maximum-price`, and `in-stock-filter` for the filters.
Place results in `search-results` using `item-card`. Use `search-next-page` and
`search-previous-page` for paging.

## Personalized recommendation hooks

Use `recommendations` for the list and `recommendation-item` for each result.

## Delivery notification hooks

Use `notifications-toggle`, `notification-item`, and `notification-unread-count`.

## Automatic reorder hooks

Use `reorder-item`, `reorder-threshold`, `reorder-quantity`, `reorder-submit`, and
`reorder-rule-item`.

## Cart recovery hooks

Use `expired-cart`, `restore-cart`, and `cart-restore-warning`.

## Recommendation feedback hooks

Use `dismiss-recommendation` inside each `recommendation-item`.
