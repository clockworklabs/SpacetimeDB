# Chat App - Full Features (22)

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

### Draft Sync

- Message drafts are saved and synced across user's devices in real-time
- Users can resume typing where they left off on any device
- Each room maintains its own draft per user
- Drafts persist across sessions until sent or cleared

**UI contract:**
- Auto-save: typing in the message input saves the draft automatically (no save button needed)
- Persistence: switching rooms and switching back restores the draft text in the message input
- Cross-session: refreshing the page restores the draft text
- Clear on send: sending a message clears the draft for that room

### Anonymous to Registered Migration

- Users can join rooms and send messages without creating an account
- Anonymous users have a temporary identity that persists for their session
- When an anonymous user registers, their identity and message history are preserved
- Room memberships and all associated data transfer to the registered account

**UI contract:**
- Guest entry: `button` with text "Guest" or "Anonymous" or "Join as Guest", OR the app auto-assigns a name like "Guest-XXXXX" or "Anon-XXXXX"
- Guest indicator: text "guest" or "anon" visible as a badge/label next to the anonymous user’s name
- Register button: `button` with text "Register" or "Sign Up" visible to guest users
- Registration form: `input` with `placeholder` containing "name" or "username" for choosing a display name
- Migration: after registration, all previous messages show the new display name

### Pinned Messages

- Users can pin important messages in a channel (admins and message authors can pin)
- Pinned messages show a pin indicator in the message list
- A "Pinned Messages" panel accessible from the channel header shows all pins for that channel
- Users can unpin messages
- Pin/unpin actions sync to all users in the channel in real-time

**UI contract:**
- Pin button: `button` with text "Pin" or `aria-label` containing "pin" visible on message hover
- Pinned indicator: text "pinned" or a pin icon (📌) visible on pinned messages
- Pinned panel: `button` with text "Pinned" or "Pins" in the channel header, opening a panel listing all pinned messages
- Unpin: `button` with text "Unpin" on pinned messages (in the panel or on hover)

### User Profiles

- Users can edit their profile: display name, bio/status message, and avatar URL
- Clicking on a username anywhere in the app opens a profile card/popover showing their info
- When a user updates their profile, the changes propagate everywhere in real-time — message attributions, member lists, online user lists, and DM headers all reflect the new name/avatar immediately
- Profile changes are visible to all users across all channels without page refresh

**UI contract:**
- Profile edit: `button` with text "Edit Profile" or "Profile" or a settings/gear icon accessible from the sidebar
- Bio input: `input` or `textarea` with `placeholder` containing "bio" or "status" (case-insensitive)
- Profile card: clicking a username opens a popover/modal showing the user’s name, bio, and avatar
- Name propagation: changing display name updates all message attributions in real-time

### @Mentions and Notification Feed

- Users can @mention other users in messages by typing `@username`
- Mentioned usernames are highlighted/styled in the message text
- When a user is mentioned, a notification is created for them
- Notification bell icon in the sidebar/header shows unread notification count
- Clicking the bell opens a notification panel listing all notifications (mentions, invites, etc.) with the source message and channel
- Users can mark individual notifications as read, or mark all as read
- Notifications update in real-time — new mentions appear instantly in the bell count
- Clicking a notification navigates to the source message in its channel

**UI contract:**
- Mention highlighting: `@username` text in messages is visually distinct (bold, colored, or wrapped in a styled `span`)
- Notification bell: `button` with text "🔔" or aria-label containing "notification" visible in the sidebar or header
- Unread count: a numeric badge near the bell showing unread notification count
- Notification panel: clicking the bell shows a list of notifications with message text and channel name
- Mark read: `button` with text "Mark Read" or "Mark All Read" in the notification panel

### Bookmarked/Saved Messages

- Users can bookmark any message for personal reference (bookmark icon on hover)
- A "Saved Messages" panel in the sidebar shows all bookmarked messages across all channels
- Each bookmark shows the message content, sender, channel name, and timestamp
- Users can remove bookmarks
- Bookmarks are personal — only visible to the user who saved them
- Bookmark list updates in real-time (e.g., if a bookmarked message is edited, the bookmark reflects the change)

**UI contract:**
- Bookmark button: `button` with text "Bookmark" or "Save" or `aria-label` containing "bookmark" or "save" visible on message hover
- Saved panel: `button` with text "Saved" or "Bookmarks" in the sidebar, opening a panel
- Bookmark entry: each saved message shows the message text and the channel/sender it came from
- Remove: `button` with text "Remove" or "Unsave" next to bookmarked messages in the panel

### Message Forwarding

- Users can forward a message to another channel they're a member of
- A "Forward" button appears on message hover, opening a channel picker
- The forwarded message appears in the target channel with a "Forwarded from #original-channel by @user" attribution
- The original message is not modified — forwarding creates a copy
- Forwarded messages appear in real-time for all members of the target channel

**UI contract:**
- Forward button: `button` with text "Forward" or `aria-label` containing "forward" visible on message hover
- Channel picker: a list or dropdown showing channel names the user can forward to
- Attribution: forwarded messages display text containing "Forwarded" or "forwarded from"
- Original unchanged: the source message has no "forwarded" indicator

### Slow Mode

- Admins can enable slow mode on a channel with a configurable cooldown (e.g., 10s, 30s, 1m, 5m)
- When slow mode is active, users can only send one message per cooldown period
- The UI shows a countdown timer after sending a message, disabling the input until the cooldown expires
- A "Slow Mode" indicator is visible in the channel header when active
- Admins are exempt from slow mode restrictions
- Slow mode setting changes sync to all channel members in real-time

**UI contract:**
- Settings: `button` with text "Settings" or a gear icon in the room header (admin only)
- Slow mode toggle: `input[type="checkbox"]` or `button` with text/label containing "Slow Mode"
- Cooldown input: `input[type="number"]` or `select` for setting the cooldown duration in seconds
- Indicator: text "Slow Mode" visible in the channel header when active
- Enforcement: after sending, the message input is `disabled` or shows countdown text until cooldown expires
- Admin exempt: admins can send messages without cooldown restriction

### Polls

- Users can create a poll in a channel with a question and 2-6 options
- Each user can vote for one option (single-choice) — no double voting
- Vote counts update in real-time for all users in the channel as votes come in
- Users can change their vote (previous vote is removed, new vote is added atomically)
- The poll creator can close the poll, preventing further votes
- Show who voted for each option (voter names visible on hover or in a detail view)

**UI contract:**
- Create poll: `button` with text "Poll" or "Create Poll" accessible from the message area
- Question input: `input` or `textarea` with `placeholder` containing "question" (case-insensitive)
- Option inputs: multiple `input` elements with `placeholder` containing "option" or "choice" (case-insensitive)
- Vote: clicking an option `button` or `label` casts a vote
- Vote count: each option shows a numeric vote count that updates in real-time
- Close poll: `button` with text "Close" or "End Poll" visible to the poll creator
- Closed state: text "Closed" or "Ended" visible on closed polls
- Voter names: `title` attribute or expandable section showing who voted for each option
