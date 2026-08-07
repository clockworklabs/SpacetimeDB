# Level 1 — Basic chat

Create a **real-time chat app**.


## UI & Style Guide

### Layout
- **Sidebar** (left, ~220px fixed): app title/branding, user info with status, room list, online users
- **Main area** (right, flex): room header bar, scrollable message list, input bar pinned to bottom
- **Panels** (right slide-in or overlay): threads, pinned messages, profiles, settings

### Visual Design
- Dark theme using the brand colors from the language section below
- Background: darkest shade for main bg, slightly lighter for sidebar and cards
- Text: light on dark, muted color for timestamps and secondary info
- Borders: subtle 1px, low contrast against background
- Consistent spacing scale (8/12/16/24px)
- Font: system font stack, clear hierarchy (bold headers, regular body, small muted metadata)
- Rounded corners on inputs, buttons, cards, and message containers

### Components
- **Messages**: sender name (colored) + timestamp (muted) + text. Group consecutive messages from same sender. Action buttons appear on hover only (which buttons depend on the features below).
- **Inputs**: full-width, rounded, subtle border, placeholder text, focus ring using primary color
- **Buttons**: filled with primary color for main actions, outlined/ghost for secondary. Clear hover and active states.
- **Badges**: small pill-shaped with count, contrasting color (e.g., unread count on rooms)
- **Modals/panels**: slide-in from right with subtle backdrop, or dropdown overlays
- **Status indicators**: small colored dots (green=online, yellow=away, red=DND, grey=offline)
- **Room list**: room names with optional icon prefix (#), active room highlighted, unread badge

### Interaction & UX
- Show loading/connecting state while backend connects (spinner or skeleton, not blank screen)
- Empty states: helpful text when no rooms, no messages, no results ("Create a room to get started")
- Error feedback: inline error messages or toast notifications, never silent failures
- Smooth transitions: fade/slide for panels, modals, and state changes
- Hover reveals: message action buttons, tooltips on reactions, user profile cards
- Keyboard support: Enter to send messages, Escape to close modals/panels
- Auto-scroll to newest message, with scroll-to-bottom button when scrolled up

## Features

### Accounts

- A visitor can **create an account** with a username and password
- A returning visitor can **sign in** with those credentials
- A signed-in user can **sign out**, returning to the signed-out state
- A signed-in session **persists across a page reload** — a returning user is not asked to
  sign in again — and across the connection dropping and re-establishing. The same person
  stays the same account throughout, keeping their rooms, messages and authorship.
- Usernames are unique. Signing up with a taken username fails with a visible error and must
  never sign the visitor in as the existing account.
- Signing in with a wrong password fails with a visible error.
- A user's account name is shown on their messages.

Identity is an account, not a display name: knowing someone's username must never grant
access to it.

### Basic Chat Features

- Users can create chat rooms and join/leave them
- Users can send messages to rooms they've joined
- Show who's online
- Include reasonable validation (e.g., don't let users spam, enforce sensible limits)


### Typing Indicators

- Show when other users are currently typing in the SAME room (typing must be scoped to room — do not broadcast typing to users in different rooms)
- Typing indicator should automatically expire after a few seconds of inactivity
- Display "User is typing..." or "Multiple users are typing..." in the UI


### Read Receipts

- Track which users have seen which messages
- Display "Seen by X, Y, Z" under messages — only show OTHER users who have seen it, not the sender
- Update read status in real-time as users view messages


### Unread Message Counts

- Show unread message count badges on the room list
- Track last-read position per user per room
- Update counts in real-time as new messages arrive or are read

### Room Members and Roles

- Each room shows a **member panel**: everyone who has joined it, with their online state
- The panel updates live as people join, leave, connect and disconnect
- The user who created a room is its **owner**; the panel shows who that is
- An owner can **promote** another member to owner, and can **remove** a member from the room
- Only members can read a room's messages or post to it. A non-member is refused by the
  server, not merely prevented by hiding a button.
- **Removal takes effect immediately.** A removed user stops receiving that room's messages
  without reloading, loses access to its history, and cannot rejoin a room they were removed
  from unless an owner re-adds them.

### Direct Messages

- A user can open a **direct conversation** with another user from that user's entry in the
  online list or a member panel. It reads and composes like a room.
- A direct conversation is visible only to its two participants. Nobody else can read it —
  not through the interface, not through any request the client can make, and it is never
  sent to anybody else's browser in the first place.
- Direct messages arrive live, are attributed, and persist like room messages
- Unread counts cover direct conversations too

### Reactions

- Users can react to a message with an emoji, and remove their own reaction
- Each message shows each emoji with a **count** of how many users have reacted with it
- A user may react at most once per emoji per message — reacting again does not raise the
  count by two
- Counts update live for everyone in the room, and every client shows the same count

### Pinned Messages

- Any member of a room can **pin** a message
- Only the user who pinned a message, or an owner of the room, can unpin it
- Pinned messages are listed for the room, visible to everyone in it
- A room may have **at most 3 pinned messages**. An attempt to pin a fourth is rejected with
  a visible error and changes nothing.
- Unpinning frees a slot
- The pinned list updates live, and every client in the room shows the same list

### Ownership

Every action belongs to somebody, and the server decides who may take it:

- Only the author of a message can edit or delete it; edited messages show as edited
- Only the pinner or a room owner can unpin
- Only a room owner can promote or remove members
- An attempt by the wrong user is refused with a visible error and changes nothing. Enforce
  this on the server — hiding a control in the interface is not enforcement.

### Back office

**Your app does not own its data.** Real chat services have other systems
writing to the same database: moderation tooling, compliance takedowns,
data-retention jobs. The app must remain correct — and its open pages current —
when data changes by a path its server never saw. This is a property of your
architecture, not a feature to bolt on.

As the concrete instance of that property, ship a script at
`scripts/backoffice.mjs` (run with `node`, from the app directory) supporting:

```
node scripts/backoffice.mjs purge-user <username>
```

It deletes every message that user has sent **by writing to the database
directly** — it must work even when the web server is not running, and it must
not talk to the web server. Every open browser must reflect the purge like any
other change: the messages disappear live, without a reload.

### Limits

- Enforce the message rate limit **on the server**: at most one message per second per
  account, with a visible error when exceeded
- The limit applies to the account, not to a browser tab — the same account in two tabs
  shares one budget
- Limits and presence are properties of the application, not of a running process: they must
  still be correct after the backend restarts
