# Bug Report — Level 7 (Rich User Presence)

## Bug: Closing one of several tabs marks the user offline while another connection is still active

**Category:** Feature Broken (presence correctness)

**Symptom:** Open the app in two tabs of the *same* browser profile (same logged-in
identity → two SpacetimeDB connections). Close one tab. The user immediately shows as
**offline** to everyone, even though the other tab is still connected and active.

**Expected:** A user stays **online** as long as they have at least one active connection.
They should be marked offline only when their **last** connection disconnects.

**Root cause:** `onDisconnect` (`clientDisconnected`) in `backend/spacetimedb/src/index.ts`
unconditionally sets `online: false` for the identity on every connection close, without
checking whether the identity still has other active connections. SpacetimeDB fires
`clientDisconnected` per connection — a single Identity can hold multiple connections
(ConnectionIds) — so closing one tab fires it even though another connection remains.

**Expected fix (mirrors the mongo fix, which tracked sockets per user):** Track active
connections per identity. Suggested approach: a table keyed by the connection id storing
that connection's identity. On `clientConnected`, insert the connection row and set the user
online. On `clientDisconnected`, delete the row for the disconnecting connection, then mark
the user `online: false` **only if** no connection rows remain for that identity. Keep
updating `lastActiveAt` on disconnect as today. (Use the disconnecting connection's id from
the reducer context.)

**Test to verify:** Two tabs, same browser profile, same user. Close one tab → the user must
remain **online** for the other tab / other users. Close the last tab → user shows offline.
Single-tab behavior (connect → online, close → offline) must still work.
