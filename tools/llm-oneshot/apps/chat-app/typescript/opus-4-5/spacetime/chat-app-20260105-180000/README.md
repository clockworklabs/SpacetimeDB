# SpacetimeDB Chat App

A Discord-like real-time chat application built with SpacetimeDB and React.

## Features

### Basic Chat
- User display names
- Create/join/leave chat rooms
- Send messages in rooms
- Online user presence

### Typing Indicators
- Real-time typing indicators
- Auto-expire after 5 seconds of inactivity
- Shows "User is typing..." or "Multiple users are typing..."

### Read Receipts
- Track which users have seen messages
- Display "Seen by X, Y, Z" under messages
- Real-time updates

### Unread Message Counts
- Unread badges on room list
- Track last-read position per user per room
- Real-time count updates

### Scheduled Messages
- Schedule messages for future delivery
- View pending scheduled messages
- Cancel scheduled messages before they send

### Ephemeral/Disappearing Messages
- Send messages that auto-delete (1 min, 5 min, or 1 hour)
- Countdown indicator showing time remaining
- Permanently deleted from database when expired

### Message Reactions
- React with emoji (👍 ❤️ 😂 😮 😢 🔥 🎉 💯)
- Toggle reactions on/off
- See who reacted on hover
- Real-time reaction updates

### Message Editing with History
- Edit your own messages
- "(edited)" indicator on edited messages
- View edit history (📜 button)
- Real-time edit sync

### Real-Time Permissions
- Room creators are admins
- Admins can kick/ban users
- Admins can promote others to admin
- Instant permission changes

### Rich User Presence
- Status options: online, away, do-not-disturb, invisible
- "Last active X minutes ago" for offline users
- Auto-set to "away" after 5 minutes of inactivity

### Message Threading
- Reply to specific messages to create threads
- Reply count and preview on parent messages
- Thread view to see all replies

### Private Rooms and DMs
- Create private (invite-only) rooms
- Invite users by username
- Direct messages between two users
- Accept/decline invitations

## Project Structure

```
chat-app-20260105-180000/
├── backend/spacetimedb/     # SpacetimeDB server module
│   ├── src/
│   │   ├── schema.ts        # Table definitions
│   │   ├── reducers.ts      # Business logic
│   │   └── index.ts         # Module entrypoint
│   ├── package.json
│   └── tsconfig.json
├── client/                   # React frontend
│   ├── src/
│   │   ├── module_bindings/ # Generated bindings
│   │   ├── App.tsx          # Main component
│   │   ├── main.tsx         # Entry point
│   │   ├── config.ts        # Connection config
│   │   └── styles.css       # Styling
│   ├── index.html
│   ├── package.json
│   └── vite.config.ts
└── README.md
```

## Deployment

### 1. Start SpacetimeDB server
```bash
spacetime start
```

### 2. Publish the module
```bash
spacetime publish chat-app --clear-database -y --project-path backend/spacetimedb
```

### 3. Generate client bindings
```bash
mkdir -p client/src/module_bindings
spacetime generate --lang typescript --out-dir client/src/module_bindings --project-path backend/spacetimedb
```

### 4. Install client dependencies and run
```bash
cd client
npm install
npm run dev
```

The app will be available at http://localhost:5173

## Architecture

- **Backend**: SpacetimeDB TypeScript module handles all data and business logic
- **Frontend**: React with SpacetimeDB React SDK for real-time subscriptions
- **Real-time**: All updates are pushed instantly to connected clients
- **Scheduling**: Uses SpacetimeDB scheduled reducers for ephemeral messages, typing expiry, and scheduled messages
