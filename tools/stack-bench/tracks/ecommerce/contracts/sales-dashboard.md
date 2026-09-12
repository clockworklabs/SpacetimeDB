# Sales dashboard application interface

Open the administrator area with `admin-link`. If category totals are on a
separate tab or screen, expose `sales-link` there to open them. Omit this control
when the totals are already shown.

Use `category-row` for each product category, including a category with no sales yet, whose
units and revenue read 0. Use `category-units` and `category-revenue` inside each row. Use `recommended-list` for signed-out
best sellers and `recommended-item` for each item. Use `buy-now` inside an `item-card` to create
sales activity for the dashboard. Each best seller contains its one-based `recommendation-rank`.
