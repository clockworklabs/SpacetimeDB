# Chat App - Discord-like Real-time Chat with SpacetimeDB

A full-featured Discord-like chat application built with SpacetimeDB and React.

## Features

### Basic Chat

- User display names
- Create, join, and leave chat rooms
- Real-time messaging
- Online user presence

### Typing Indicators

- Real-time "User is typing..." indicators
- Auto-expires after 5 seconds of inactivity

### Read Receipts

- "Seen by X, Y, Z" under messages
- Real-time updates as users view messages

### Unread Message Counts

- Badge counts on room list
- Tracks last-read position per user per room

### Scheduled Messages

- Schedule messages to send at a future time
- View and cancel pending scheduled messages

### Ephemeral/Disappearing Messages

- Messages that auto-delete after a set duration (1 min, 5 min, 1 hour)
- Visual countdown indicator

### Message Reactions

- React with emojis: 👍 ❤️ 😂 😮 😢 👎 🎉 🔥
- Toggle reactions on/off
- See who reacted on hover

### Message Editing with History

- Edit your own messages
- "(edited)" indicator
- View edit history by clicking the indicator

### Real-Time Permissions

- Room creators are admins
- Kick/ban users from rooms
- Promote users to admin
- Instant permission updates

### Rich User Presence

- Status options: Online, Away, Do Not Disturb, Invisible
- "Last active X minutes ago" for offline users
- Auto-away after 5 minutes of inactivity

### Message Threading

- Reply to specific messages
- View reply count and thread panel
- Nested conversation support

### Private Rooms and DMs

- Create private/invite-only rooms
- Invite users by username
- Direct messages (DMs) between two users
- Accept/decline invitations

## Project Structure

```
chat-app-20260102-171317/
├── backend/
│   └── spacetimedb/
│       ├── src/
│       │   ├── schema.ts      # Table definitions
│       │   └── index.ts       # Reducers and lifecycle handlers
│       ├── package.json
│       └── tsconfig.json
├── client/
│   ├── src/
│   │   ├── main.tsx          # Entry point with SpacetimeDB provider
│   │   ├── App.tsx           # Main application component
│   │   ├── styles.css        # Dark theme styles
│   │   └── module_bindings/  # Generated bindings (regenerate after publish)
│   ├── index.html
│   ├── package.json
│   ├── tsconfig.json
│   └── vite.config.ts
└── README.md
```

## Getting Started

### Prerequisites

- Node.js 18+
- SpacetimeDB CLI

### Backend Setup

1. Start SpacetimeDB:

   ```bash
   spacetime start
   ```

2. Publish the module:

   ```bash
   cd backend/spacetimedb
   spacetime publish chat-app --module-path .
   ```

3. Generate client bindings:
   ```bash
   spacetime generate --lang typescript --out-dir ../../client/src/module_bindings --module-path .
   ```

### Client Setup

1. Install dependencies:

   ```bash
   cd client
   npm install
   ```

2. Start the development server:

   ```bash
   npm run dev
   ```

3. Open http://localhost:3000 in your browser

## Architecture

### Backend (SpacetimeDB)

**Tables:**

- `user` - User profiles and presence
- `room` - Chat rooms (public, private, DMs)
- `room_member` - Room memberships with admin status
- `banned_user` - Banned users per room
- `room_invitation` - Pending room invitations
- `message` - Chat messages with threading support
- `message_edit` - Edit history
- `message_reaction` - Emoji reactions
- `read_receipt` - Per-message read receipts
- `typing_indicator` - Active typing indicators
- `scheduled_message` - Scheduled messages (scheduled table)
- `scheduled_message_view` - User-visible scheduled messages
- `ephemeral_cleanup_job` - Cleanup jobs for expiring messages
- `presence_away_job` - Auto-away scheduling

**Reducers:**

- User: `set_name`, `set_status`, `update_activity`
- Rooms: `create_room`, `join_room`, `leave_room`
- DMs: `start_dm`
- Invitations: `invite_to_room`, `respond_to_invitation`
- Permissions: `kick_user`, `ban_user`, `unban_user`, `promote_to_admin`
- Messages: `send_message`, `send_ephemeral_message`, `edit_message`, `delete_message`
- Reactions: `toggle_reaction`
- Typing: `start_typing`, `stop_typing`
- Read status: `mark_messages_read`
- Scheduling: `schedule_message`, `cancel_scheduled_message`

### Client (React)

- Dark theme with consistent color palette
- Responsive sidebar with room/DM tabs
- Real-time updates via SpacetimeDB subscriptions
- Connection handling with token persistence
- Activity heartbeat for presence tracking

## UI Features

- Dark theme with Discord-inspired color palette
- Hover effects and focus indicators
- Loading and empty states
- Modal dialogs for scheduling
- Thread panel for replies
- Reaction picker on hover
- Unread badges
- Status indicators

## Notes

- The module bindings in `client/src/module_bindings` are placeholder files. After publishing the backend, regenerate them with `spacetime generate`.
- Remove `<React.StrictMode>` from the React app as it interferes with WebSocket connections.
- Connection tokens are persisted in localStorage for session continuity.
