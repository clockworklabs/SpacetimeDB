# PostgreSQL Chat App - Private Rooms and DMs

A Discord-like real-time chat application built with PostgreSQL, Express, Socket.io, and React.

## Features

### Basic Chat

- User display names
- Create and join chat rooms
- Real-time messaging
- Online user list with presence

### Typing Indicators

- Shows when users are typing
- Auto-expires after 5 seconds of inactivity
- "Multiple users are typing..." support

### Read Receipts

- Track which users have seen messages
- Click "Seen" to view who read a message
- Real-time read status updates

### Unread Message Counts

- Badge counts on room list
- Per-user, per-room tracking
- Clears when viewing room

### Scheduled Messages

- Schedule messages for future delivery
- View pending scheduled messages
- Cancel scheduled messages before they send

### Ephemeral/Disappearing Messages

- Set messages to auto-delete (1, 5, 15, or 30 minutes)
- Live countdown timer display
- Permanent deletion from database

### Message Reactions

- React with emoji (👍 ❤️ 😂 😮 😢)
- Toggle reactions on/off
- View who reacted on hover

### Message Editing with History

- Edit your own messages
- "(edited)" indicator
- Click to view full edit history

### Real-Time Permissions

- Room creators are admins
- Kick/ban users (immediate effect)
- Promote users to admin
- Banned users lose access instantly

### Rich User Presence

- Status: online, away, do-not-disturb, invisible
- "Last active X minutes ago" display
- Auto-away after 5 minutes of inactivity

### Message Threading

- Reply to specific messages
- Thread panel shows all replies
- Reply count on parent messages

### Private Rooms and Direct Messages

- Create private/invite-only rooms
- Invite users by username search
- Accept/decline invitations
- Direct messages between two users
- Only members can see private content

## Tech Stack

- **Backend:** Node.js, Express, Socket.io, Drizzle ORM, PostgreSQL
- **Frontend:** React, TypeScript, Vite, Socket.io-client
- **Database:** PostgreSQL 16

## Local Development

### Prerequisites

- Node.js 20+
- PostgreSQL running locally (or Docker)

### Server Setup

```bash
cd server
npm install
# Set DATABASE_URL if not using default
npm run db:push  # Create tables
npm run dev      # Start server on port 3001
```

### Client Setup

```bash
cd client
npm install
npm run dev      # Start client on port 5174
```

### Environment Variables

**Server:**

- `DATABASE_URL` - PostgreSQL connection string (default: `postgres://postgres:postgres@localhost:5432/chat-app`)
- `JWT_SECRET` - JWT signing secret (default: `chat-app-20260104-160000-secret`)
- `PORT` - Server port (default: `3001`)

## Docker Deployment

```bash
# Start all services
docker-compose up --build -d

# View logs
docker-compose logs -f server

# Stop services
docker-compose down

# Clean restart (removes data)
docker-compose down -v
docker-compose up --build -d
```

**Ports:**

- PostgreSQL: 5432
- Server API: 3001
- Client: 5174

## API Endpoints

### Authentication

- `POST /api/auth/register` - Register with display name

### Users

- `GET /api/users/me` - Get current user
- `PATCH /api/users/status` - Update status
- `GET /api/users/search?q=` - Search users
- `GET /api/users/online` - Get online users

### Rooms

- `GET /api/rooms` - List accessible rooms
- `POST /api/rooms` - Create room
- `POST /api/rooms/dm` - Create DM
- `POST /api/rooms/:id/join` - Join room
- `POST /api/rooms/:id/leave` - Leave room
- `GET /api/rooms/:id/members` - Get members
- `POST /api/rooms/:id/invite` - Invite user
- `POST /api/rooms/:id/kick` - Kick user
- `POST /api/rooms/:id/ban` - Ban user
- `POST /api/rooms/:id/promote` - Promote to admin

### Messages

- `GET /api/rooms/:id/messages` - Get messages
- `POST /api/rooms/:id/messages` - Send message
- `PATCH /api/messages/:id` - Edit message
- `GET /api/messages/:id/history` - Get edit history
- `GET /api/messages/:id/replies` - Get thread replies
- `POST /api/messages/:id/reactions` - Toggle reaction
- `POST /api/rooms/:id/read` - Mark as read
- `GET /api/messages/:id/receipts` - Get read receipts

### Invitations

- `GET /api/invitations` - Get pending invitations
- `POST /api/invitations/:id/respond` - Accept/decline

### Scheduled/Ephemeral

- `GET /api/rooms/:id/scheduled` - Get scheduled messages
- `DELETE /api/messages/:id/scheduled` - Cancel scheduled

### Unread

- `GET /api/unread` - Get unread counts

## Socket Events

### Emitted by Server

- `room:created`, `message:created`, `message:updated`, `message:deleted`
- `reaction:added`, `reaction:removed`
- `typing:started`, `typing:stopped`
- `user:online`, `user:offline`, `user:status`
- `member:joined`, `member:left`, `member:kicked`, `member:banned`, `member:promoted`
- `room:kicked`, `room:banned`
- `invitation:received`

### Emitted by Client

- `typing:start`, `typing:stop`
- `room:join`, `room:leave`
- `heartbeat`
