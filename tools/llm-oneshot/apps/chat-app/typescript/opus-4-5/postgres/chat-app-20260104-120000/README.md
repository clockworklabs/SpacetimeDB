# Chat App - PostgreSQL Implementation

A Discord-like real-time chat application built with PostgreSQL, Express, Socket.io, and React.

## Features

- **Basic Chat**: Display names, rooms, messages, online status
- **Typing Indicators**: Real-time typing status with auto-expiry
- **Read Receipts**: Track who has seen messages
- **Unread Counts**: Badge counts on room list
- **Scheduled Messages**: Send messages at a future time
- **Ephemeral Messages**: Self-destructing messages
- **Reactions**: Emoji reactions with toggle and user list
- **Message Editing**: Edit messages with full history
- **Permissions**: Admin roles, kick, ban, promote
- **Rich Presence**: Online/away/DND/invisible status with auto-away
- **Threading**: Reply to messages and view threaded conversations
- **Private Rooms**: Invite-only rooms and direct messages

## Tech Stack

- **Database**: PostgreSQL 16
- **Backend**: Node.js, Express, Socket.io, Drizzle ORM
- **Frontend**: React, Vite, TypeScript
- **Authentication**: JWT

## Development

### Prerequisites

- Node.js 20+
- PostgreSQL 16+ (or Docker)

### Local Development

1. Start PostgreSQL (or use Docker):

   ```bash
   docker run -d --name postgres -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=chat-app -p 5432:5432 postgres:16-alpine
   ```

2. Start the server:

   ```bash
   cd server
   npm install
   npm run db:push
   npm run dev
   ```

3. Start the client:

   ```bash
   cd client
   npm install
   npm run dev
   ```

4. Open http://localhost:5174

### Docker Compose

```bash
docker-compose up --build -d
```

Then open http://localhost:5174

## API Endpoints

### Authentication

- `POST /api/auth/register` - Register new user

### Users

- `GET /api/users/me` - Get current user
- `PATCH /api/users/me` - Update display name
- `PATCH /api/users/me/status` - Update status
- `GET /api/users` - Get all users
- `GET /api/users/search?q=` - Search users

### Rooms

- `GET /api/rooms` - Get rooms
- `POST /api/rooms` - Create room
- `POST /api/dms` - Create DM
- `POST /api/rooms/:id/join` - Join room
- `POST /api/rooms/:id/leave` - Leave room
- `GET /api/rooms/:id/members` - Get members
- `POST /api/rooms/:id/invite` - Invite user
- `POST /api/rooms/:id/kick/:userId` - Kick user
- `POST /api/rooms/:id/ban/:userId` - Ban user
- `POST /api/rooms/:id/promote/:userId` - Promote to admin

### Invitations

- `GET /api/invitations` - Get pending invitations
- `POST /api/invitations/:id/accept` - Accept invitation
- `POST /api/invitations/:id/decline` - Decline invitation

### Messages

- `GET /api/rooms/:id/messages` - Get messages
- `POST /api/rooms/:id/messages` - Send message
- `PATCH /api/messages/:id` - Edit message
- `GET /api/messages/:id/history` - Get edit history
- `GET /api/messages/:id/replies` - Get thread replies
- `GET /api/rooms/:id/scheduled` - Get scheduled messages
- `DELETE /api/messages/:id/scheduled` - Cancel scheduled

### Reactions

- `POST /api/messages/:id/reactions` - Toggle reaction
- `GET /api/messages/:id/reactions` - Get reactions

### Read Receipts

- `POST /api/rooms/:id/read` - Mark messages as read
- `GET /api/rooms/unread` - Get unread counts

## Socket Events

### Client → Server

- `room:join` - Join room for updates
- `room:leave` - Leave room
- `typing:start` - Start typing
- `typing:stop` - Stop typing
- `activity` - Report user activity

### Server → Client

- `user:online` - User came online
- `user:offline` - User went offline
- `user:status` - User status changed
- `user:updated` - User profile updated
- `room:created` - Room created
- `room:member:joined` - Member joined
- `room:member:left` - Member left
- `room:member:kicked` - Member kicked
- `room:member:banned` - Member banned
- `room:member:promoted` - Member promoted
- `room:kicked` - You were kicked
- `room:banned` - You were banned
- `message:created` - New message
- `message:updated` - Message edited
- `message:deleted` - Message deleted (ephemeral)
- `thread:reply` - New thread reply
- `reaction:added` - Reaction added
- `reaction:removed` - Reaction removed
- `messages:read` - Messages marked as read
- `typing:start` - Someone started typing
- `typing:stop` - Someone stopped typing
- `invitation:received` - You received an invitation

## Ports

- PostgreSQL: 5432
- Server: 3001
- Client: 5174
