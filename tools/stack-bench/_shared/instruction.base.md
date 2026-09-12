<!--
  Shared, backend-agnostic body of the realtime-chat task instruction.
  Each backend's instruction.md = this body + a backend-specific "Contract" section
  that pins the exact identifiers the grader connects to. Keep this file as the
  single source of truth for the app spec; edit the per-backend Contract sections
  only for naming/transport specifics.
-->

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

An automated grader connects two clients via the backend's real-time client SDK
and checks: real-time delivery, message ordering, sender attribution, and
history persistence for a late-joining client. Grading is behavioral — it drives
the running app through its client SDK; it does not inspect your source. Partial
credit is awarded per check.

Your implementation MUST expose exactly the identifiers named in the
**Contract** section below so the grader can connect.
