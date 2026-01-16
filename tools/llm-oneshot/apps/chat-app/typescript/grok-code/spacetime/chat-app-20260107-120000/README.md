# SpacetimeDB Discord-like Chat App

A real-time chat application built with SpacetimeDB featuring message editing with history, reactions, typing indicators, read receipts, scheduled messages, and ephemeral messages.

## Features

### Core Chat Features
- **Real-time messaging** with WebSocket connections
- **User display names** and online status
- **Room creation and joining** (public rooms)
- **Rate limiting** (5 messages per minute per user per room)

### Message Editing with History ✨
- **Edit your own messages** within 5 minutes of sending
- **View edit history** with timestamps and previous content
- **"(edited)" indicator** on modified messages
- **Full audit trail** of all message changes

### Advanced Features
- **Emoji reactions** with real-time updates
- **Typing indicators** ("User is typing...")
- **Read receipts** ("Seen by X, Y, Z")
- **Unread message counts** with badges
- **Scheduled messages** (send messages at future times)
- **Ephemeral messages** (auto-delete after set duration)

## Project Structure

```
chat-app-20260107-120000/
├── backend/spacetimedb/
│   ├── src/
│   │   ├── schema.ts    # Database tables and relationships
│   │   └── index.ts     # Reducers and business logic
│   ├── package.json
│   └── tsconfig.json
└── client/
    ├── src/
    │   ├── components/     # React components
    │   │   ├── App.tsx
    │   │   ├── Sidebar.tsx
    │   │   ├── ChatArea.tsx
    │   │   ├── MessageItem.tsx  # Message editing UI
    │   │   ├── MessageInput.tsx
    │   │   └── UserSetup.tsx
    │   ├── module_bindings/  # Generated SpacetimeDB types
    │   ├── config.ts
    │   ├── main.tsx
    │   └── index.css
    ├── package.json
    ├── tsconfig.json
    └── vite.config.ts
```

## Setup and Running

### Prerequisites
- Node.js 18+
- SpacetimeDB CLI (`spacetime`)

### 1. Start SpacetimeDB Server
```bash
spacetime start
```

### 2. Publish the Backend Module
```bash
cd backend/spacetimedb
spacetime publish chat-app --project-path .
```

### 3. Generate Client Bindings
```bash
spacetime generate --lang typescript --out-dir ../client/src/module_bindings --project-path .
```

### 4. Start the Client
```bash
cd ../client
npm run dev
```

The app will be available at `http://localhost:3000`

## Message Editing How-To

1. **Send a message** in any chat room
2. **Hover over your message** to see the "Edit" button
3. **Click "Edit"** to enter edit mode
4. **Modify the message** and press Enter or click "Save"
5. **View edit history** by clicking the arrow next to "(edited)"
6. **See all changes** with timestamps and previous content

## Configuration

Set the SpacetimeDB server URL in `client/src/config.ts`:

```typescript
export const CONFIG = {
  SPACETIMEDB_URI: 'ws://your-server:3000',
  MODULE_NAME: 'chat-app',
};
```

## Database Schema

### Key Tables for Message Editing

- **`message`** - Main messages with edit tracking
- **`message_edit`** - History of all message edits
- **`user`** - User information and display names
- **`room`** - Chat rooms
- **`room_member`** - Room membership and read positions

### Edit History Tracking

When a message is edited:
1. Original content is saved to `message_edit` table
2. Message is updated with new content and `editedAt` timestamp
3. `isEdited` flag is set to true
4. Edit history is displayed with timestamps

## Troubleshooting

### Common Issues

1. **"Could not resolve 'spacetimedb/server'"**
   - Run `npm install` in the backend directory

2. **WebSocket connection fails**
   - Ensure `spacetime start` is running
   - Check the `SPACETIMEDB_URI` in config

3. **Bindings not generated**
   - Run the generate command from the backend directory
   - Ensure the module is published first

4. **StrictMode breaks WebSocket**
   - The app removes React.StrictMode automatically

### Development Tips

- Use the browser dev tools to inspect WebSocket connections
- Check the SpacetimeDB logs with `spacetime logs chat-app`
- All database operations are transactional and deterministic

## Architecture Notes

- **No external state management** - SpacetimeDB subscriptions drive all UI updates
- **Real-time by default** - All changes sync instantly across clients
- **Type-safe** - Generated bindings ensure type safety
- **Transactional** - All operations are ACID compliant