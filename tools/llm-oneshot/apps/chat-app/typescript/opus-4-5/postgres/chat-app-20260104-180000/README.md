# Chat App - PostgreSQL

A Discord-like real-time chat application built with PostgreSQL, Express, Socket.io, and React.

## Features

### Basic Chat
- User registration with display names
- Create and join chat rooms (public/private)
- Real-time messaging
- Online user presence

### Typing Indicators
- Real-time typing indicators
- Auto-expire after 5 seconds of inactivity
- Shows "User is typing..." or "Multiple users are typing..."

### Read Receipts
- Track who has seen each message
- "Seen by X, Y, Z" display under messages
- Real-time updates as users view messages

### Unread Message Counts
- Badge counts on room list
- Per-user per-room tracking
- Real-time updates

### Scheduled Messages
- Schedule messages for future delivery
- View and cancel pending scheduled messages
- Messages appear at scheduled time

### Ephemeral/Disappearing Messages
- 1-minute or 5-minute auto-delete options
- Countdown timer display
- Permanently deleted from database when expired

### Message Reactions
- 5 emoji reactions: 👍 ❤️ 😂 😮 😢
- Toggle reactions on/off
- Hover to see who reacted

### Message Editing with History
- Edit your own messages
- "(edited)" indicator shown
- Click to view edit history

### Real-Time Permissions
- Room creators are admins
- Admins can kick/ban users
- Admins can promote users to admin
- Instant permission updates

### Rich User Presence
- Status: online, away, dnd, invisible
- Auto-away after 5 minutes of inactivity
- Real-time status sync

### Message Threading
- Reply to specific messages
- Thread view with reply count
- Real-time thread updates

### Private Rooms & DMs
- Create private/invite-only rooms
- Invite users by username
- Direct messages between two users
- Accept/decline invitations

## Tech Stack

- **Database:** PostgreSQL with Drizzle ORM
- **Backend:** Express.js + Socket.io
- **Frontend:** React + Vite + TypeScript
- **Auth:** JWT tokens

## Quick Start

### Docker (Recommended)

```bash
docker-compose up --build
```

Open http://localhost:5174

### Local Development

1. **Start PostgreSQL** (port 5432)

2. **Server:**
```bash
cd server
npm install
npm run db:push
npm run dev
```

3. **Client:**
```bash
cd client
npm install
npm run dev
```

Open http://localhost:5174

## API Endpoints

### Auth
- `POST /api/auth/register` - Register new user
- `GET /api/auth/me` - Get current user
- `PUT /api/auth/displayName` - Update display name
- `PUT /api/auth/status` - Update status

### Rooms
- `GET /api/rooms` - List public rooms
- `GET /api/rooms/my` - List user's rooms
- `POST /api/rooms` - Create room
- `POST /api/rooms/:id/join` - Join room
- `POST /api/rooms/:id/leave` - Leave room
- `GET /api/rooms/:id/members` - Get room members
- `POST /api/rooms/:id/kick/:userId` - Kick user
- `POST /api/rooms/:id/ban/:userId` - Ban user
- `POST /api/rooms/:id/promote/:userId` - Promote to admin
- `POST /api/rooms/:id/invite` - Invite user
- `GET /api/rooms/unread` - Get unread counts

### Messages
- `GET /api/rooms/:id/messages` - Get messages
- `POST /api/rooms/:id/messages` - Send message
- `PUT /api/messages/:id` - Edit message
- `GET /api/messages/:id/history` - Get edit history
- `GET /api/messages/:id/thread` - Get thread replies
- `POST /api/messages/:id/reactions` - Toggle reaction
- `POST /api/messages/:id/read` - Mark as read

### DMs & Invites
- `POST /api/dm` - Create DM
- `GET /api/invites` - Get pending invites
- `POST /api/invites/:id/accept` - Accept invite
- `POST /api/invites/:id/decline` - Decline invite

## Socket.io Events

### Client → Server
- `typing:start` - Start typing
- `typing:stop` - Stop typing
- `activity` - User activity ping
- `room:join` - Join room channel
- `room:leave` - Leave room channel

### Server → Client
- `user:updated` - User status/info changed
- `room:created` - New room created
- `room:member:joined/left/kicked/banned/promoted` - Member changes
- `room:invite` - New room invite
- `dm:created` - New DM created
- `message:created/updated/deleted` - Message changes
- `message:reactions:updated` - Reaction changes
- `message:read` - Read receipts updated
- `message:thread:updated` - Thread reply count changed
- `typing:update` - Typing indicator changed
