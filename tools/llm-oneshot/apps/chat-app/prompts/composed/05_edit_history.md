# Chat App - Edit History

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

### Scheduled Messages

* Users can compose a message and schedule it to send at a future time
* Show pending scheduled messages to the author (with option to cancel)
* Message appears in the room at the scheduled time

### Ephemeral/Disappearing Messages

* Users can send messages that auto-delete after a set duration (e.g., 1 minute, 5 minutes)
* Show a countdown or indicator that the message will disappear
* Message is permanently deleted from the database when time expires

### Message Reactions

* Users can react to messages with emoji (e.g., 👍 ❤️ 😂 😮 😢)
* Show reaction counts on messages that update in real-time
* Users can toggle their own reactions on/off
* Display who reacted when hovering over reaction counts

### Message Editing with History

* Users can edit their own messages after sending
* Show "(edited)" indicator on edited messages
* Other users can view the edit history of a message
* Edits sync in real-time to all viewers
