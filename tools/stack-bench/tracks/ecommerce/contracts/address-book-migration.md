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
- `address-add` opens an empty editor. `address-edit` opens the selected entry.
  Use `address-name-input`, `address-text-input`, `address-save` and
  `address-cancel` in the editor.
- On the book, `data-submit-state` is `idle`, `pending`, `succeeded` or `failed`
  for the most recent write. Mark success only after server acknowledgement.
  Display write errors in `address-error`.

Use normal authenticated application requests. Entry IDs are opaque strings;
their format and storage representation are not prescribed.
