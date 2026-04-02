# Chat App - Basic

Create a **real-time chat app**.

**See `language/*.md` for language-specific setup, architecture, and constraints.**

## UI Requirements

See language file for branding and color scheme.

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

- Show when other users are currently typing in a room
- Typing indicator should automatically expire after a few seconds of inactivity
- Display "User is typing..." or "Multiple users are typing..." in the UI

**UI contract:**
- Typing text: visible text containing "typing" (case-insensitive) when another user types
- Auto-expiry: typing indicator text disappears within 6 seconds of inactivity

### Read Receipts

- Track which users have seen which messages
- Display "Seen by X, Y, Z" under messages (or a seen indicator)
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
