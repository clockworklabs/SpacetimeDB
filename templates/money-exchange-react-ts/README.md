# Money Exchange

A small React and TypeScript demo for a hackathon: every participant receives a
private account, claims a public nickname, and transfers money to other named
participants in real time.

The example demonstrates:

- Private identity-owned accounts and an automatic starter balance
- Atomic transfers implemented as a SpacetimeDB reducer
- Private account changes represented as credit and debit entries
- A public recipient directory without exposing other users' balances

## Run The Template

Create and run the app with the SpacetimeDB CLI:

```bash
spacetime dev --template money-exchange-react-ts
```

Open [http://localhost:5173](http://localhost:5173), then open a second
private browser window to create another identity and send payments between
the two users.

## Explore The Code

The server module is in `spacetimedb/src/index.ts`. On first connection it
creates a private account containing `$100.00`. Users must claim a unique
nickname before they appear in the recipient directory.

The `transfer` reducer accepts a recipient identity and a cent amount. It
validates ownership and available funds, debits the sender, credits the
recipient, and writes a `Debit` account change for the sender and a `Credit`
account change for the recipient in one transaction. Errors abort the whole
transaction, so a failed payment never partially changes balances or history.

The `account` and `account_change` tables are private. The `my_account` and
`my_account_changes` views let each connected identity subscribe only to its
own balance and change history. The public `directory` table contains names
and identities for choosing whom to pay.

The React client is in `src/App.tsx`; generated type-safe bindings live in
`src/module_bindings`.

## Extend It

This example uses play money. Natural hackathon extensions include payment
memos, payment requests, shared wallets, an administrator faucet, or
authenticated user profiles.
