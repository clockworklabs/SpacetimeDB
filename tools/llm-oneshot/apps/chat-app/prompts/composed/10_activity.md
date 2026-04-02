# Chat App - Activity Indicators

Create a **real-time chat app**.


## UI & Style Guide

### Layout
- **Sidebar** (left, ~220px fixed): app title/branding, user info with status, room list, online users
- **Main area** (right, flex): room header bar, scrollable message list, input bar pinned to bottom
- **Panels** (right slide-in or overlay): threads, pinned messages, profiles, settings

### Visual Design
- Dark theme using the brand colors from the language section below
- Background: darkest shade for main bg, slightly lighter for sidebar and cards
- Text: light on dark, muted color for timestamps and secondary info
- Borders: subtle 1px, low contrast against background
- Consistent spacing scale (8/12/16/24px)
- Font: system font stack, clear hierarchy (bold headers, regular body, small muted metadata)
- Rounded corners on inputs, buttons, cards, and message containers

### Components
- **Messages**: sender name (colored) + timestamp (muted) + text. Group consecutive messages from same sender. Action buttons (edit, react, reply, pin, forward, bookmark) appear on hover only.
- **Inputs**: full-width, rounded, subtle border, placeholder text, focus ring using primary color
- **Buttons**: filled with primary color for main actions, outlined/ghost for secondary. Clear hover and active states.
- **Badges**: small pill-shaped with count, contrasting color (e.g., unread count on rooms)
- **Modals/panels**: slide-in from right with subtle backdrop, or dropdown overlays
- **Status indicators**: small colored dots (green=online, yellow=away, red=DND, grey=offline)
- **Room list**: room names with optional icon prefix (#), active room highlighted, unread badge

### Interaction & UX
- Show loading/connecting state while backend connects (spinner or skeleton, not blank screen)
- Empty states: helpful text when no rooms, no messages, no results ("Create a room to get started")
- Error feedback: inline error messages or toast notifications, never silent failures
- Smooth transitions: fade/slide for panels, modals, and state changes
- Hover reveals: message action buttons, tooltips on reactions, user profile cards
- Keyboard support: Enter to send messages, Escape to close modals/panels
- Auto-scroll to newest message, with scroll-to-bottom button when scrolled up

## Features

**Important:** Each feature below includes a "UI contract" section specifying required element attributes for automated testing. You MUST follow these — they define the user-facing interface. Your architecture, state management, and backend design are entirely up to you.

### Basic Chat Features

- Users can set a display name
- Users can create chat rooms and join/leave them
- Users can send messages to rooms they've joined
- Show who's online
- Include reasonable validation (e.g., don't let users spam, enforce sensible limits)

**UI contract:**
- Name input: `placeholder` contains "name" (case-insensitive)
- Name submit: `button` with text "Join", "Register", "Set Name", or `type="submit"`
- Room creation: `button` with text containing "Create" or "New" or "+"
- Room name input: `placeholder` contains "room" or "name" (case-insensitive)
- Message input: `placeholder` contains "message" (case-insensitive)
- Send message: pressing Enter in the message input sends the message
- Room list: room names visible as clickable text in a sidebar or list
- Join room: clicking room name joins/enters it, or a `button` with text "Join"
- Leave room: `button` with text "Leave"
- Online users: user names displayed as text in a visible user list or member panel

### Typing Indicators

- Show when other users are currently typing in a room
- Typing indicator should automatically expire after a few seconds of inactivity
- Display "User is typing..." or "Multiple users are typing..." in the UI

**UI contract:**
- Typing text: visible text containing "typing" (case-insensitive) when another user types
- Auto-expiry: typing indicator text disappears within 6 seconds of inactivity

### Read Receipts

- Track which users have seen which messages
- Display "Seen by X, Y, Z" under messages (or a seen indicator)
- Update read status in real-time as users view messages

**UI contract:**
- Receipt text: text containing "seen" or "read" (case-insensitive) appears near messages after another user views them
- Reader names: the receipt text includes the viewing user’s display name

### Unread Message Counts

- Show unread message count badges on the room list
- Track last-read position per user per room
- Update counts in real-time as new messages arrive or are read

**UI contract:**
- Badge: a visible numeric badge (e.g., "3") appears next to room names in the sidebar when there are unread messages
- Badge clears when the room is opened/entered

### Scheduled Messages

- Users can compose a message and schedule it to send at a future time
- Show pending scheduled messages to the author (with option to cancel)
- Message appears in the room at the scheduled time

**UI contract:**
- Schedule button: `button` with text "Schedule" or `aria-label` containing "schedule", or an icon button with `title` containing "schedule"
- Time picker: an `input[type="datetime-local"]` or `input[type="time"]` or `input[type="number"]` for setting the send time
- Pending list: text "Scheduled" or "Pending" visible when viewing scheduled messages
- Cancel: `button` with text "Cancel" next to pending scheduled messages

### Ephemeral/Disappearing Messages

- Users can send messages that auto-delete after a set duration (e.g., 1 minute, 5 minutes)
- Show a countdown or indicator that the message will disappear
- Message is permanently deleted from the database when time expires

**UI contract:**
- Ephemeral toggle: `select`, `button`, or `input` with text/label containing "ephemeral", "disappear", or "expire" (case-insensitive)
- Duration options: selectable durations (e.g., 30s, 1m, 5m)
- Indicator: visible text containing a countdown, "expires", or "disappearing" on ephemeral messages
- Deletion: the message text is removed from the DOM after the duration expires

### Message Reactions

- Users can react to messages with emoji (e.g., 👍 ❤️ 😂 😮 😢)
- Show reaction counts on messages that update in real-time
- Users can toggle their own reactions on/off
- Display who reacted when hovering over reaction counts

**UI contract:**
- Reaction trigger: `button` with emoji text (👍 ❤️ 😂 😮 😢) or a `button` with text "React" / aria-label containing "react" visible on message hover
- Reaction display: emoji + count (e.g., "👍 2") visible below or beside the reacted message
- Toggle: clicking the same emoji again removes the user’s reaction
- Hover info: `title` attribute on reaction element showing voter names

### Message Editing with History

- Users can edit their own messages after sending
- Show "(edited)" indicator on edited messages
- Other users can view the edit history of a message
- Edits sync in real-time to all viewers

**UI contract:**
- Edit button: `button` with text "Edit" visible on hover over own messages
- Edit form: an inline `input` or `textarea` replaces the message content during editing, with a "Save" `button`
- Edited indicator: text "(edited)" visible on edited messages
- History: clicking "(edited)" opens a view showing previous versions of the message

### Real-Time Permissions

- Room creators are admins and can kick/ban users from their rooms
- Kicked users immediately lose access and stop receiving room updates
- Admins can promote other users to admin
- Permission changes apply instantly without requiring reconnection

**UI contract:**
- Admin indicator: text "Admin" or "ADMIN" visible for admin users in the member list
- Members panel: `button` with text "Members" or "Manage" in the room header
- Kick button: `button` with text "Kick" next to non-admin members
- Promote button: `button` with text "Promote" next to non-admin members
- Kicked feedback: kicked user sees text containing "kicked" or is redirected away from the room

### Rich User Presence

- Users can set their status: online, away, do-not-disturb, invisible
- Show "Last active X minutes ago" for users who aren't online
- Status changes sync to all viewers in real-time
- Auto-set to "away" after period of inactivity

**UI contract:**
- Status selector: `select` or group of `button` elements with text "Online", "Away", "Do Not Disturb" / "DND", "Invisible"
- Status indicator: colored dot or icon next to user names (green=online, yellow=away, red=DND, grey=invisible)
- Last active: text containing "Last active" or "ago" for offline/away users

### Message Threading

- Users can reply to specific messages, creating a thread
- Show reply count and preview on parent messages
- Threaded view to see all replies to a message
- New replies sync in real-time to thread viewers

**UI contract:**
- Reply button: `button` with text "Reply" or "💬" visible on message hover
- Reply count: text like "N replies" or "💬 N" visible on messages that have replies
- Thread panel: clicking the reply button/count opens a panel showing the parent message and all replies
- Thread input: `input` or `textarea` with `placeholder` containing "reply" (case-insensitive) in the thread panel

### Private Rooms and Direct Messages

- Users can create private/invite-only rooms that don't appear in the public room list
- Room creators can invite specific users by username
- Direct messages (DMs) between two users as a special type of private room
- Invited users receive notifications and can accept/decline invitations
- Only members can see private room content and member lists

**UI contract:**
- Private toggle: `input[type="checkbox"]` or `button` with text/label containing "Private" during room creation
- Private indicator: text "private" or a lock icon (🔒) visible on private rooms in the sidebar
- Invite button: `button` with text "Invite" in the room header or members panel
- Invitation UI: invited user sees text containing the room name with "Accept" and "Decline" `button` elements
- DM button: `button` with text "DM" or "💬" next to user names in the user list

### Room Activity Indicators

- Show activity badges on rooms with recent message activity (e.g., "Active now", "Hot")
- Display real-time message velocity or activity level per room
- Activity indicators update live as conversation pace changes
- Help users quickly identify where active conversations are happening

**UI contract:**
- Active badge: text "Active" or "ACTIVE" (green) visible on rooms with 1+ messages in the last 5 minutes
- Hot badge: text "Hot" or "🔥" (orange) visible on rooms with 5+ messages in the last 2 minutes
- Badges appear in the room list/sidebar next to room names
