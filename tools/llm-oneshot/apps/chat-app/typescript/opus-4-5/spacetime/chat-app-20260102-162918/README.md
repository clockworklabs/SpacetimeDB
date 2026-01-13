# Discord-like Chat App with SpacetimeDB

A full-featured real-time chat application built with SpacetimeDB and React.

## Features

- **Basic Chat**: Create/join rooms, send messages, see who's online
- **Typing Indicators**: Real-time "user is typing..." with auto-expiration
- **Read Receipts**: Track and display who has seen messages
- **Unread Message Counts**: Badge notifications for unread messages
- **Scheduled Messages**: Compose and schedule messages for later
- **Ephemeral Messages**: Self-destructing messages with countdown
- **Message Reactions**: Emoji reactions (👍 ❤️ 😂 😮 😢 🎉 🔥 👀)
- **Message Editing**: Edit messages with full history tracking
- **Real-Time Permissions**: Admin controls (kick/ban/promote) with instant effect
- **Rich User Presence**: Status (online/away/DND/invisible) with auto-away
- **Message Threading**: Reply to messages in threads
- **Private Rooms**: Invite-only rooms with invitation system
- **Direct Messages**: Private DM rooms between two users

## Project Structure

```
chat-app-20260102-162918/
├── backend/
│   └── spacetimedb/
│       ├── package.json
│       ├── tsconfig.json
│       └── src/
│           ├── schema.ts      # Table definitions
│           └── index.ts       # Reducers
├── client/
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── index.html
│   └── src/
│       ├── main.tsx          # App entry point
│       ├── App.tsx           # Main app component
│       ├── styles.css        # Global styles
│       ├── module_bindings.ts # SpacetimeDB bindings (generated)
│       └── components/
│           ├── ChatArea.tsx
│           ├── CreateRoomModal.tsx
│           ├── EditHistoryModal.tsx
│           ├── InvitesPanel.tsx
│           ├── MembersPanel.tsx
│           ├── MessageInput.tsx
│           ├── MessageItem.tsx
│           ├── RoomSettingsModal.tsx
│           ├── ScheduledMessagesPanel.tsx
│           ├── Sidebar.tsx
│           ├── StartDmModal.tsx
│           ├── StatusDropdown.tsx
│           ├── ThreadPanel.tsx
│           └── UserSetup.tsx
└── README.md
```

## Deployment

### 1. Start SpacetimeDB

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
spacetime generate --lang typescript --out-dir ../../client/src --project-path .
```

### 4. Install Client Dependencies

```bash
cd ../../client
npm install
```

### 5. Run the Client

```bash
npm run dev
```

The app will be available at http://localhost:3000

## Backend Tables

| Table | Purpose |
|-------|---------|
| `user` | User profiles with presence status |
| `room` | Chat rooms (public, private, DM) |
| `room_member` | Room membership with admin flag |
| `room_ban` | Banned users per room |
| `room_invite` | Pending room invitations |
| `message` | Chat messages with threading support |
| `message_edit` | Edit history for messages |
| `message_reaction` | Emoji reactions on messages |
| `read_receipt` | Track who has seen which messages |
| `typing_indicator` | Real-time typing status |
| `typing_expiry` | Scheduled cleanup for typing indicators |
| `scheduled_message` | Messages scheduled for future delivery |
| `ephemeral_message_cleanup` | Scheduled cleanup for disappearing messages |
| `auto_away_check` | Scheduled auto-away status updates |

## Key Reducers

| Reducer | Purpose |
|---------|---------|
| `set_name` | Set user display name |
| `set_status` | Update presence status |
| `create_room` | Create public/private room |
| `join_room` / `leave_room` | Room membership |
| `invite_to_room` / `respond_to_invite` | Private room invitations |
| `kick_user` / `ban_user` / `promote_to_admin` | Admin controls |
| `start_dm` | Create direct message room |
| `send_message` | Send regular message |
| `send_ephemeral_message` | Send disappearing message |
| `schedule_message` / `cancel_scheduled_message` | Scheduled messages |
| `reply_to_message` | Thread replies |
| `edit_message` / `delete_message` | Message management |
| `toggle_reaction` | Add/remove emoji reactions |
| `start_typing` / `stop_typing` | Typing indicators |
| `mark_messages_read` | Update read receipts |
