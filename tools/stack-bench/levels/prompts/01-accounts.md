# Level 1 — Accounts and a live shared resource

Build a real-time chat application.

## UI & Style Guide

### Layout
- **Sidebar** (left, ~220px fixed): app title, signed-in user, room list, online users
- **Main area** (right, flex): room header, scrollable message list, input bar pinned to bottom

### Visual design
- Dark theme, light text on dark background, muted colour for timestamps and secondary text
- Subtle 1px borders, consistent 8/12/16/24px spacing scale, rounded corners on inputs,
  buttons, cards and messages
- System font stack with clear hierarchy: bold headers, regular body, small muted metadata

### Interaction
- Show a connecting/loading state rather than a blank screen
- Empty states with helpful text ("Create a room to get started")
- Errors surface in the UI — never fail silently
- Enter sends a message; auto-scroll to the newest message

## Features

### Accounts

- A visitor can **create an account** with a username and password
- A returning visitor can **sign in** with those credentials
- A signed-in user can **sign out**, which returns them to the signed-out state
- A signed-in session **persists across a page reload** — a returning user is not asked to
  sign in again
- The session also survives the connection dropping and re-establishing, and the backend
  restarting. The same person must remain the same account throughout, keeping their rooms,
  their messages and their authorship.
- Usernames are unique. Signing up with a taken username fails with a visible error, and
  must never sign the visitor in as the existing account.
- Signing in with a wrong password fails with a visible error.
- A user's display name is shown on their messages.

Identity is an account, not a display name: two people who happen to choose similar names
are different accounts, and knowing someone's username must never grant access to it.

### Rooms

- A signed-in user can **create a room** by name
- All users can see the list of rooms; the list updates live when someone creates one
- A user can **enter** a room to read and post, and **leave** a room
- The sidebar shows which users are currently online, updating live

### Messages

- A user in a room can **send a message**; it appears in that room for every user currently
  in it, live, without anyone reloading
- Messages persist: a user who reloads, reconnects, or returns after a backend restart still
  sees the room's history
- Messages are attributed to the account that sent them
- A message belongs to exactly one room and must never appear in another
- Apply reasonable validation — reject empty and over-long messages, and rate-limit a single
  account to no more than one message per 500ms with a visible error when exceeded
