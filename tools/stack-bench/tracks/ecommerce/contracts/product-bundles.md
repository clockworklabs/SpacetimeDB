# Product bundle interface

The catalog has a `bundles-link`. The bundle panel contains `bundle-card` rows with
`bundle-name`, numeric `bundle-price`, and `bundle-component` rows. Each component row has
`bundle-component-name` and numeric `bundle-component-quantity`. Each component row also
exposes its quantity in `data-quantity`.

Catalog staff use `bundle-name-input`, `bundle-price-input` (currency units), and
`bundle-components-input` (JSON array of `{ "item": "product name", "quantity": 1 }`),
then `bundle-save`. Saving an existing name edits that bundle. Each `bundle-card` exposes
`data-bundle-input` as JSON `{ "bundleId": "..." }`.
The save button exposes `data-bundle-save-input` with `{ name, price, componentsJson }`
from the current form values.

Use the same application write for the visible form and this named action:

<!-- interface:http -->
Save a bundle with `POST /api/bundles` and `{ name, price, componentsJson }`.
<!-- /interface -->

<!-- interface:reducer -->
Save a bundle with `save_bundle(name: string, price: number, componentsJson: string)`.
<!-- /interface -->

Each component uses an existing product's exact name. `componentsJson` contains the
component array as a JSON string.
