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

An automated grader connects two clients via the Convex client SDK and checks:
real-time delivery, message ordering, sender attribution, and history
persistence for a late-joining client. Grading is behavioral — it drives the
running app through the SDK; it does not inspect your source. Partial credit is
awarded per check.

## Contract (Convex)

Build a Convex app and deploy it so the grader can connect via `CONVEX_URL`
(set in this environment).

- **Table `messages`** with fields, named exactly: `sender` (string),
  `text` (string), `sentAt` (number, ms since epoch).
- **Mutation `messages:sendMessage`** taking `{ sender: string, text: string }`,
  inserting one `messages` row with `sentAt` set to the current time. It must
  throw for empty `text`.
- **Query `messages:listMessages`** taking no args, returning all messages in
  ascending send order. This query must be reactive (the grader subscribes via
  `client.onUpdate(api.messages.listMessages, {})`).

The exact `messages` table, `sendMessage`, and `listMessages` names are required
— the grader depends on them.

You have 1800 seconds to complete this task. Do not cheat by using online
solutions or hints specific to this task.
