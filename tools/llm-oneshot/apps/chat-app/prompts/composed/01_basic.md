# Chat App - Basic

Create a **real-time chat app**.

**See `language/*.md` for language-specific setup, architecture, and constraints.**

## UI Requirements

Use SpacetimeDB brand styling (dark theme).

## Features

### Basic Chat Features

* Users can set a display name
* Users can create chat rooms and join/leave them
* Users can send messages to rooms they've joined
* Show who's online
* Include reasonable validation (e.g., don't let users spam, enforce sensible limits)

### Typing Indicators

* Show when other users are currently typing in a room
* Typing indicator should automatically expire after a few seconds of inactivity
* Display "User is typing..." or "Multiple users are typing..." in the UI

### Read Receipts

* Track which users have seen which messages
* Display "Seen by X, Y, Z" under messages (or a seen indicator)
* Update read status in real-time as users view messages

### Unread Message Counts

* Show unread message count badges on the room list
* Track last-read position per user per room
* Update counts in real-time as new messages arrive or are read
