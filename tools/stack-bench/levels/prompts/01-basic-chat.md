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

