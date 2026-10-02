# Catalog management application interface

Use `admin-link` to open the administrator area containing the product controls.
If these controls are on a separate tab or screen within it, expose `catalog-management-link`
there to open them. Omit this control when the product controls are already shown.
Use `catalog-name`, `catalog-category`, `catalog-price`, and `catalog-variants` for the product
values; `catalog-category` accepts a new category name as text, and `catalog-variants` accepts
comma-separated variant names. Use `catalog-save` to add the product. Use `item-variant` for each named variant shown
to a visitor.
