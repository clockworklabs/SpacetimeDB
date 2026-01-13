# SpacetimeDB Chat App - Message Editing with History

A Discord-like real-time chat application using SpacetimeDB as the backend and .NET MAUI for the GUI client.

## Features

- **Real-time messaging** with SpacetimeDB subscriptions
- **Message editing** with full edit history tracking
- **Typing indicators** with automatic cleanup
- **Read receipts** per message
- **Unread message counts** per room
- **Scheduled messages** (send later)
- **Ephemeral messages** (disappearing)
- **Message reactions** with emoji
- **Multi-room support**
- **User online/offline status**

## Prerequisites

- .NET 8.0 SDK
- SpacetimeDB CLI (`spacetime`)
- MAUI workload: `dotnet workload install maui-windows`

## Setup

### 1. Start SpacetimeDB server

```bash
spacetime start
```

### 2. Publish the backend module

```bash
cd backend
dotnet publish -c Release
spacetime publish chat-app --bin-path bin/Release/net8.0/wasi-wasm/AppBundle/backend.wasm
```

Or to clear existing data:

```bash
spacetime publish chat-app --clear-database -y --bin-path bin/Release/net8.0/wasi-wasm/AppBundle/backend.wasm
```

### 3. Generate client bindings (if needed)

```bash
spacetime generate --lang csharp --out-dir client/module_bindings --bin-path backend/bin/Release/net8.0/wasi-wasm/AppBundle/backend.wasm
```

### 4. Run the MAUI client

```bash
cd client
dotnet run -f net8.0-windows10.0.19041.0
```

## Project Structure

```
chat-app-YYYYMMDD-HHMMSS/
├── backend/
│   ├── backend.csproj       # SpacetimeDB module project
│   ├── global.json          # .NET SDK version pinning
│   ├── Lib.cs               # Table definitions
│   └── Module.cs            # Reducers and lifecycle hooks
├── client/
│   ├── client.csproj        # MAUI client project
│   ├── App.xaml             # Application resources (dark theme)
│   ├── App.xaml.cs          # Application entry
│   ├── MainPage.xaml        # Main chat UI (XAML)
│   ├── MainPage.xaml.cs     # Chat logic and SpacetimeDB connection
│   ├── MauiProgram.cs       # MAUI configuration
│   └── module_bindings/     # Generated SpacetimeDB bindings
└── README.md
```

## Usage

1. **Set display name**: Click the name field in the bottom-left and type your name, press Enter
2. **Create a room**: Type a room name in the top-left field and click +
3. **Join a room**: Click any room in the sidebar
4. **Send messages**: Type in the message box and press Enter or click Send
5. **Edit messages**: Click ✏️ on your own messages
6. **View edit history**: Click 📜 on edited messages
7. **Add reactions**: Click 😀 on any message
8. **Schedule message**: Click ⏰, enter delay in seconds, then message
9. **Send ephemeral**: Click 💨, enter lifetime in seconds, then message

## Architecture

### Backend (SpacetimeDB Module)

- **Tables**: User, Room, RoomMember, Message, MessageEdit, TypingIndicator, ReadReceipt, LastRead, Reaction, ScheduledMessage, EphemeralCleanup, TypingCleanup
- **Reducers**: All CRUD operations plus scheduled callbacks
- **Scheduled Tables**: Auto-send scheduled messages, auto-delete ephemeral messages, auto-cleanup typing indicators

### Client (MAUI)

- Uses `SpacetimeDB.ClientSDK` for real-time connection
- `FrameTick()` called via `IDispatcherTimer` on main thread
- Row callbacks update ObservableCollections for automatic UI binding
- Token persistence for identity preservation across sessions

## Dark Theme

The app uses a Discord-inspired dark theme with:
- Background colors: #1e1f22, #2b2d31, #313338
- Accent: #5865f2 (Discord blue)
- Success: #23a559, Warning: #f0b232, Error: #da373c
