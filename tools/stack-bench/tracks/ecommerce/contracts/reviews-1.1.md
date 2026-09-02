# Review application interface

Use `review-rating` for a one-to-five rating, `review-input` for the comment, and
`review-submit` to submit it. Put the item's numeric server identifier in
`data-review-item-id` on `review-submit`. Use `review-average` for the numeric average,
`review-item` for each visible review, and `review-error` for a failed submission.

Expose the same review operation used by `review-submit`.

<!-- interface:http -->
Use `POST /api/items/:id/reviews`, where `:id` is the item identifier. Send `rating` and
`comment` in the request body.
<!-- /interface -->

<!-- interface:reducer -->
Use the `submit_review` reducer with the item identifier, rating, and comment.
<!-- /interface -->
