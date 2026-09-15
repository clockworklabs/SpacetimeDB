# Address-book application interface

Keep the existing profile interface. From the signed-in profile, expose
`address-book-link` to open the address book. Use the existing `data-role`
convention for these controls. A panel that blocks navigation must provide
`overlay-close`.

- `address-book` contains the current customer's entries. Set `data-loaded` to
  `true` only after the initial authorized read finishes. An empty loaded book
  has no entries. Show a read error separately; do not render it as an empty book.
- Each `address-entry` has its stable `data-address-id` and `data-default` set
  to `true` or `false`. It contains `address-name` and `address-text` as saved
  text, plus `address-edit`, `address-delete` and `address-default` controls.
  Its `data-address-params` contains the JSON operation parameters `{ "id": "..." }`.
- `address-add` opens an empty editor. `address-edit` opens the selected entry.
  Use `address-name-input`, `address-text-input`, `address-save` and
  `address-cancel` in the editor.
- On the book, `data-submit-state` is `idle`, `pending`, `succeeded` or `failed`
  for the most recent write. Mark success only after server acknowledgement.
  Display write errors in `address-error`.

Use normal authenticated application requests. Entry IDs are opaque strings;
their format and storage representation are not prescribed.

Keep the existing account, catalog, stock, cart, order, payment and warehouse
records queryable through their existing storage names and fields. Preserve
their IDs and field types. Additional storage for the address book is unrestricted.

The UI and other clients use the same owner-scoped operations below. Use the
current customer's session for every read and write. Do not accept a customer ID
from the caller to select the owner. A read returns all entries for that owner,
without pagination or filtering. Text is returned exactly as saved. IDs are
unique within the book. Read errors remain errors, rather than empty results.

<!-- interface:http -->
Use these HTTP operations:

- `GET /api/addresses` returns `{ "entries": [{ "id": "...", "name": "...", "address": "...", "isDefault": true }] }`.
- `POST /api/addresses` adds an entry from `{ "name": "...", "address": "..." }`.
- `PUT /api/addresses/:id` edits its name and address from that same body.
- `PUT /api/addresses/:id/default` selects the default.
- `DELETE /api/addresses/:id` removes the entry.
<!-- /interface -->

<!-- interface:reducer -->
Expose the authenticated `my_addresses` view with columns
`id`, `name`, `address` (strings), and `is_default` (boolean). It returns only
the current identity's entries, including when the same session opens another
connection. Use reducers `open_address_book()` to open the book,
`add_address(name, address)`, `edit_address(id, name, address)`,
`choose_address(id)` and `delete_address(id)`. These parameters are strings;
the underlying storage may use another ID type.
<!-- /interface -->

If existing profiles are converted when first opened, opening the book must
finish that conversion before reporting it as loaded. Unauthenticated reads
must be refused or return no entries. Unauthenticated writes and attempts to
write another owner's entry must be refused. Refusals must not disclose private
address data.
