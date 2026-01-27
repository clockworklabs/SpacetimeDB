# Discord-like Chat App with SpacetimeDB

A real-time chat application built with SpacetimeDB, featuring Discord-like functionality including message editing with history.

## Features

### Core Chat Features

- **Real-time messaging** - Messages appear instantly for all users
- **User presence** - See who's online, away, or do-not-disturb
- **Display names** - Users can set custom display names
- **Room management** - Create public or private rooms, join/leave rooms

### Advanced Features

- **Message editing with history** - Edit messages and view full edit history
- **Message reactions** - React to messages with emoji
- **Read receipts** - Track message read status
- **Typing indicators** - See when others are typing
- **Unread message counts** - Badge counters for unread messages per room
- **Scheduled messages** - Schedule messages to send later
- **Ephemeral messages** - Self-destructing messages (1 or 5 minutes)
- **Room invitations** - Invite users to private rooms
- **Admin/moderator roles** - Room management permissions

## Project Structure

```
chat-app-20260107-120000/
├── backend/spacetimedb/     # SpacetimeDB module
│   ├── src/
│   │   ├── index.ts        # Module entry point
│   │   ├── schema.ts       # Database schema
│   │   └── reducers.ts     # Business logic
│   ├── package.json
│   └── tsconfig.json
└── client/                 # React frontend
    ├── src/
    │   ├── App.tsx         # Main chat UI
    │   ├── main.tsx        # App entry point
    │   ├── config.ts       # Configuration
    │   ├── styles.css      # Discord-like styling
    │   └── module_bindings/# Generated client bindings
    ├── package.json
    ├── tsconfig.json
    ├── vite.config.ts
    └── index.html
```

## Running the Application

### Prerequisites

- SpacetimeDB installed and running
- Node.js and npm

### 1. Start SpacetimeDB

```bash
spacetime start
```

### 2. Publish the backend module

```bash
cd backend/spacetimedb
spacetime publish chat-app --clear-database -y --project-path .
```

### 3. Install dependencies

```bash
# Backend
cd backend/spacetimedb
npm install

# Frontend
cd ../../client
npm install
```

### 4. Start the frontend

```bash
cd client
npm run dev
```

The app will be available at `http://localhost:5173`

## Message Editing with History

The app implements Discord-like message editing with full history tracking:

- **Edit messages**: Click the ✏️ icon on your own messages to edit them
- **View edit history**: Click "(edited)" on any edited message to see the full history
- **History preservation**: All edits are stored and can be viewed chronologically
- **Permissions**: Users can only edit their own messages
- **Real-time updates**: Edits sync instantly to all viewers

## Technical Implementation

### Backend (SpacetimeDB)

- **Schema**: Tables for users, rooms, messages, message edits, reactions, etc.
- **Reducers**: Business logic for all chat operations
- **Real-time sync**: Automatic data synchronization via SpacetimeDB subscriptions

### Frontend (React + TypeScript)

- **State management**: SpacetimeDB tables provide reactive state
- **UI components**: Discord-inspired dark theme and layout
- **WebSocket connection**: Real-time updates via SpacetimeDB client
- **Responsive design**: Works on desktop and mobile

## Key SpacetimeDB Concepts Used

- **Tables with relationships**: Foreign keys and indexes for efficient queries
- **Reducers**: Transactional operations that mutate data
- **Subscriptions**: Real-time data synchronization
- **Identity system**: User authentication and authorization
- **Scheduled operations**: For ephemeral messages and scheduled sending
- **Public views**: Automatic data visibility management

## Architecture Decisions

- **Dark theme**: Matches Discord's aesthetic
- **Real-time first**: All features prioritize real-time updates
- **Permission-based**: Clear separation of user roles and permissions
- **Extensible reactions**: Easy to add more emoji reactions
- **Ephemeral design**: Messages that need to disappear are handled server-side
- **Edit history**: Complete audit trail of message modifications
