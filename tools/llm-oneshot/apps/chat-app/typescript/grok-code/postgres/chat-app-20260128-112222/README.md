# SpacetimeDB Chat App

A real-time chat application built with TypeScript, PostgreSQL, Node.js, Express, Socket.io, and React.

## Features

### Basic Chat Features

- User authentication with display names
- Create and join chat rooms
- Real-time messaging
- Online user tracking
- Message validation and spam prevention

### Advanced Features

- **Typing Indicators**: Show when users are typing in real-time
- **Read Receipts**: Track which users have seen messages
- **Unread Message Counts**: Display badges on room list
- **Scheduled Messages**: Compose messages to send at future times
- **Ephemeral Messages**: Messages that auto-delete after set duration
- **Message Reactions**: React to messages with emoji (👍 ❤️ 😂 😮 😢)
- **Message Editing**: Edit messages with edit history tracking

## Project Structure

```
├── server/                 # Backend Node.js/Express server
│   ├── src/
│   │   ├── db/            # Database schema and connection
│   │   ├── services/      # Background services (cleanup, scheduling)
│   │   └── socket/        # WebSocket handlers
│   ├── package.json
│   └── tsconfig.json
├── client/                 # Frontend React application
│   ├── src/
│   │   ├── components/    # React components
│   │   └── types.ts       # TypeScript interfaces
│   ├── package.json
│   └── tsconfig.json
└── README.md
```

## Setup Instructions

### Prerequisites

- Node.js 18+
- PostgreSQL 12+
- npm or yarn

### Database Setup

1. Create a PostgreSQL database named `chat-app`
2. Update the `DATABASE_URL` in `server/.env` if needed

### Backend Setup

```bash
cd server
npm install
npm run db:generate  # Generate database schema
npm run db:migrate   # Run migrations
npm run dev          # Start development server
```

### Frontend Setup

```bash
cd client
npm install
npm run dev          # Start development server
```

### Running the Application

1. Start the backend server (runs on port 3001)
2. Start the frontend server (runs on port 3000)
3. Open http://localhost:3000 in your browser

## Architecture

### Backend

- **Express.js**: REST API endpoints
- **Socket.io**: Real-time WebSocket communication
- **Drizzle ORM**: Database queries and migrations
- **PostgreSQL**: Primary database
- **TypeScript**: Type safety

### Frontend

- **React**: UI framework
- **Vite**: Build tool and development server
- **Socket.io Client**: Real-time communication
- **TypeScript**: Type safety

### Database Schema

- `users`: User accounts
- `rooms`: Chat rooms
- `room_members`: Room membership
- `messages`: Chat messages
- `message_edits`: Message edit history
- `message_reactions`: Message reactions
- `read_receipts`: Message read status
- `typing_indicators`: Typing status
- `unread_counts`: Unread message counts
- `scheduled_messages`: Scheduled message queue
- `online_users`: Online user tracking

## API Documentation

### Socket Events

#### Authentication

- `authenticate` → `{ displayName: string }`
- `authenticated` ← `{ userId: string, displayName: string }`

#### Room Management

- `create_room` → `{ name: string }`
- `room_created` ← `{ roomId: string, name: string }`
- `join_room` → `{ roomId: string }`
- `room_joined` ← `{ roomId: string, messages: Message[] }`

#### Messaging

- `send_message` → `{ roomId: string, content: string, scheduledFor?: string, expiresAt?: string }`
- `new_message` ← `Message`
- `edit_message` → `{ messageId: string, content: string }`
- `message_edited` ← `{ messageId: string, content: string, updatedAt: Date }`

#### Real-time Features

- `start_typing` → `{ roomId: string }`
- `user_typing` ← `{ userId: string, displayName: string, roomId: string }`
- `stop_typing` → `{ roomId: string }`
- `user_stopped_typing` ← `{ userId: string, roomId: string }`
- `mark_as_read` → `{ roomId: string, messageId: string }`
- `message_read` ← `{ messageId: string, userId: string, displayName: string }`
- `add_reaction` → `{ messageId: string, emoji: string }`
- `remove_reaction` → `{ messageId: string, emoji: string }`
- `reaction_updated` ← `{ messageId: string, reactions: Reaction[] }`

#### Status Updates

- `user_online` ← `{ userId: string, displayName: string }`
- `user_offline` ← `{ userId: string }`
- `unread_count_updated` ← `{ roomId: string, count: number }`

## Development

### Database Migrations

```bash
cd server
npm run db:generate  # Generate migration files
npm run db:migrate   # Apply migrations
```

### Building for Production

```bash
# Backend
cd server
npm run build
npm start

# Frontend
cd client
npm run build
npm run preview
```

## License

This project is part of the SpacetimeDB LLM benchmarking suite.
