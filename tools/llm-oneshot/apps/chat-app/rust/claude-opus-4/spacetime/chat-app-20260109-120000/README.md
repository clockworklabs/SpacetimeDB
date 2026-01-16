# SpacetimeDB Chat App with Message Editing History

A real-time chat application built with **SpacetimeDB** (Rust backend) and a **Rust TUI client**.

## Features

- **Basic Chat:** Create rooms, join/leave, send messages, see who's online
- **Typing Indicators:** See when others are typing (auto-expires after 5 seconds)
- **Read Receipts:** "Seen by X, Y, Z" displayed under messages
- **Unread Message Counts:** Shows unread count per room in the room list
- **Scheduled Messages:** Schedule messages for future delivery (up to 24 hours)
- **Ephemeral/Disappearing Messages:** Messages that auto-delete after a set duration
- **Message Reactions:** React with emoji (👍 ❤️ 😂 😮 😢 🎉)
- **Message Editing with History:** Edit messages, view edit history

## Project Structure

```
chat-app-20260109-120000/
├── backend/              # SpacetimeDB Rust module
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
├── client/               # Rust TUI client
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       └── module_bindings/  # Generated (run spacetime generate)
└── README.md
```

## Prerequisites

- [SpacetimeDB CLI](https://spacetimedb.com/install)
- Rust toolchain

## Setup & Run

### 1. Start SpacetimeDB Server

```bash
spacetime start
```

### 2. Publish the Backend Module

```bash
cd backend
spacetime publish chat-app --project-path .
```

Or with database clear:

```bash
spacetime publish chat-app --clear-database -y --project-path .
```

### 3. Generate Client Bindings

```bash
cd ../client
mkdir -p src/module_bindings
spacetime generate --lang rust --out-dir src/module_bindings --project-path ../backend
```

### 4. Run the Client

```bash
cd client
cargo run
```

## Keyboard Controls

### Global
- `q` / `Ctrl+C` - Quit
- `?` / `F1` - Show help
- `Esc` - Go back / Cancel

### Room List
- `↑` / `↓` - Navigate rooms
- `Enter` - Open selected room
- `n` - Create new room
- `j` - Join room by ID
- `s` - Set display name

### Chat View
- `↑` / `↓` - Select message
- `i` - Start typing message
- `e` - Edit selected message (your own only)
- `h` - View edit history of selected message
- `r` - React to selected message
- `m` - Mark selected message as read
- `/` or `:` - Enter command mode

## Commands

| Command | Description |
|---------|-------------|
| `/help` | Show help |
| `/name <name>` | Set your display name |
| `/create <room_name>` | Create a new room |
| `/join <room_id>` | Join a room by ID |
| `/leave` | Leave current room |
| `/ephemeral <secs> <msg>` | Send disappearing message |
| `/schedule <secs> <msg>` | Schedule message for later |
| `/cancel <id>` | Cancel a scheduled message |
| `/react <emoji>` | React to selected message |
| `/read` | Mark all messages as read |
| `/scheduled` | Show pending scheduled messages |

## View Server Logs

```bash
spacetime logs chat-app
```

## Architecture

### Backend Tables
- `user` - User profiles with online status
- `room` - Chat rooms
- `room_member` - Room membership
- `message` - Chat messages
- `message_edit` - Edit history
- `typing_indicator` - Who's typing
- `read_receipt` - Read receipts per message
- `user_room_status` - Last read position per room
- `message_reaction` - Emoji reactions
- `scheduled_message` - Pending scheduled messages
- `send_scheduled_message_job` - Scheduler for delivery
- `delete_message_job` - Scheduler for ephemeral cleanup
- `typing_cleanup_job` - Scheduler for typing indicator expiry

### Client
Built with [ratatui](https://github.com/ratatui/ratatui) for TUI rendering and the SpacetimeDB Rust SDK for real-time data synchronization.
