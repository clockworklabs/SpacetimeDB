# Chat App - Scheduled Messages

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

**Important:** Each feature below includes a "UI contract" section specifying required element attributes for automated testing. You MUST follow these — they define the user-facing interface. Your architecture, state management, and backend design are entirely up to you.

### Basic Chat Features

- Users can set a display name
- Users can create chat rooms and join/leave them
- Users can send messages to rooms they've joined
- Show who's online
- Include reasonable validation (e.g., don't let users spam, enforce sensible limits)

**UI contract:**
- Name input: `placeholder` contains "name" (case-insensitive)
- Name submit: `button` with text "Join", "Register", "Set Name", or `type="submit"`
- Room creation: `button` with text containing "Create" or "New" or "+"
- Room name input: `placeholder` contains "room" or "name" (case-insensitive)
- Message input: `placeholder` contains "message" (case-insensitive)
- Send message: pressing Enter in the message input sends the message
- Room list: room names visible as clickable text in a sidebar or list
- Join room: clicking room name joins/enters it, or a `button` with text "Join"
- Leave room: `button` with text "Leave"
- Online users: user names displayed as text in a visible user list or member panel

### Typing Indicators

- Show when other users are currently typing in the SAME room (typing must be scoped to room — do not broadcast typing to users in different rooms)
- Typing indicator should automatically expire after a few seconds of inactivity
- Display "User is typing..." or "Multiple users are typing..." in the UI

**UI contract:**
- Typing text: visible text containing "typing" (case-insensitive) when another user types
- Auto-expiry: typing indicator text disappears within 6 seconds of inactivity

### Read Receipts

- Track which users have seen which messages
- Display "Seen by X, Y, Z" under messages — only show OTHER users who have seen it, not the sender
- Update read status in real-time as users view messages

**UI contract:**
- Receipt text: text containing "seen" or "read" (case-insensitive) appears near messages after another user views them
- Reader names: the receipt text includes the viewing user’s display name

### Unread Message Counts

- Show unread message count badges on the room list
- Track last-read position per user per room
- Update counts in real-time as new messages arrive or are read

**UI contract:**
- Badge: a visible numeric badge (e.g., "3") appears next to room names in the sidebar when there are unread messages
- Badge clears when the room is opened/entered

### Scheduled Messages

- Users can compose a message and schedule it to send at a future time
- Show pending scheduled messages to the author (with option to cancel)
- Message appears in the room at the scheduled time

**UI contract:**
- Schedule button: `button` with text "Schedule" or `aria-label` containing "schedule", or an icon button with `title` containing "schedule"
- Time picker: an `input[type="datetime-local"]` or `input[type="time"]` or `input[type="number"]` for setting the send time
- Pending list: text "Scheduled" or "Pending" visible when viewing scheduled messages
- Cancel: `button` with text "Cancel" next to pending scheduled messages
