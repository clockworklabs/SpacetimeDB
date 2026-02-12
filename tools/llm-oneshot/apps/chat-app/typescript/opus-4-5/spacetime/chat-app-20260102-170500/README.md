# Chat App - Discord-like Real-Time Chat with SpacetimeDB

A full-featured Discord-like chat application built with SpacetimeDB and React.

## Features

- **Basic Chat**: Create/join rooms, send messages, see who's online
- **Private Rooms & DMs**: Create invite-only rooms and direct messages
- **Typing Indicators**: See when users are typing in real-time
- **Read Receipts**: Track who has seen messages
- **Unread Counts**: Badge showing unread messages per room
- **Scheduled Messages**: Schedule messages to send at a future time
- **Ephemeral Messages**: Self-destructing messages with countdown
- **Message Reactions**: React to messages with emoji
- **Message Editing**: Edit messages with full history tracking
- **Message Threading**: Reply to specific messages
- **Rich Presence**: Online/away/DND/invisible status
- **Permissions**: Room admins can kick users and promote others

## Deployment

### Using Docker (Recommended)

```bash
# Start SpacetimeDB
docker-compose up -d

# Wait for container to be ready, then add server
spacetime server add docker http://localhost:3000 --no-fingerprint
spacetime server set-default docker

# Install backend dependencies and publish
cd backend/spacetimedb
npm install
cd ../..
echo y | spacetime publish chat-app --clear-database --project-path backend/spacetimedb

# Generate client bindings
spacetime generate --lang typescript --out-dir client/src/module_bindings --project-path backend/spacetimedb

# Install and run client
cd client
npm install
npm run dev
```

### Using Local SpacetimeDB

```bash
# Make sure spacetime is running
spacetime start

# Install backend dependencies and publish
cd backend/spacetimedb
npm install
cd ../..
echo y | spacetime publish chat-app --clear-database --project-path backend/spacetimedb

# Generate client bindings
spacetime generate --lang typescript --out-dir client/src/module_bindings --project-path backend/spacetimedb

# Install and run client
cd client
npm install
npm run dev
```

## Project Structure

```
chat-app-20260102-170500/
├── backend/
│   └── spacetimedb/
│       ├── src/
│       │   ├── schema.ts    # Table definitions
│       │   └── index.ts     # Reducers and logic
│       ├── package.json
│       └── tsconfig.json
├── client/
│   ├── src/
│   │   ├── App.tsx          # Main application
│   │   ├── main.tsx         # Entry point
│   │   ├── index.css        # Styles
│   │   └── module_bindings/ # Generated SpacetimeDB bindings
│   ├── package.json
│   ├── tsconfig.json
│   └── vite.config.ts
├── docker-compose.yml
└── README.md
```

## Tables

- **user**: User profiles with identity, name, status, and online state
- **room**: Chat rooms (public, private, or DM)
- **room_member**: Room membership with roles (admin/member)
- **message**: Messages with optional threading and expiry
- **typing_indicator**: Real-time typing status
- **read_receipt**: Last read message per user per room
- **reaction**: Message reactions (emoji)
- **edit_history**: Message edit history
- **room_invitation**: Private room invitations
- **scheduled_message**: Messages scheduled for future delivery
- **ephemeral_cleanup**: Cleanup jobs for ephemeral messages
- **typing_cleanup**: Cleanup jobs for typing indicators

## License

MIT
