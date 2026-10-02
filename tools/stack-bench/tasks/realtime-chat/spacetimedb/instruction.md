# Task: minimal real-time chat backend

Build the backend for a minimal real-time chat application. There is a single
shared chat room. Clients connect, subscribe to the room, and send messages;
every connected client sees new messages pushed in real time, and a client that
connects later receives the full message history.

## Functional requirements

1. **Persisted messages.** Each message has a `sender` (string), a `text`
   (string), and a server-assigned send time. Messages persist for the lifetime
   of the deployment.
2. **Send.** A client can send a message by providing `sender` and `text`.
   Reject empty `text`.
3. **Real-time subscription.** A subscribed client receives every message
   currently in history at subscribe time, and every new message thereafter,
   pushed (not polled), in send order.

## What is graded

An automated grader connects two clients via the SpacetimeDB TypeScript SDK and
checks: real-time delivery, message ordering, sender attribution, and history
persistence for a late-joining client. Grading is behavioral — it drives the
running app through the SDK; it does not inspect your source. Partial credit is
awarded per check.

## Contract (SpacetimeDB)

Implement a SpacetimeDB module and publish it to the local instance so the
grader can connect to it.

- **Database name:** `chat`, published to the `local` server
  (`spacetime publish --server local chat`). A local SpacetimeDB instance is
  already running in this environment.
- **Public table `message`** with columns, named exactly:
  - `id`: `u64`, primary key, auto-increment
  - `sender`: `String`
  - `text`: `String`
  - `sent_at`: `Timestamp`
- **Reducer `send_message(sender: String, text: String)`** that inserts one
  `message` row with the given `sender` and `text`, `sent_at` set to the current
  reducer timestamp, and an auto-assigned `id`. It must return an error for
  empty `text`.
- Clients subscribe with `SELECT * FROM message`.

You may use Rust, C#, or TypeScript for the module. The reference solution is in
Rust. The exact table/column/reducer names above are required — the grader
depends on them.

You have 1800 seconds to complete this task. Do not cheat by using online
solutions or hints specific to this task.
