# Chat App Feature Tests

### 1. Basic Chat
**Seed:** `specs/seed.spec.ts`

#### 1.1 User Registration and Room Creation
**Steps:**
1. Find the name/display name input field and type "Alice"
2. Click the join/register/submit button
3. Verify "Alice" appears somewhere on the page
4. Find the room name input and type "TestRoom"
5. Click the create/add room button (may be labeled "+", "Create", or "New")
6. Verify "TestRoom" appears in the room list

#### 1.2 Messaging Between Two Users
**Steps:**
1. Click on "TestRoom" to enter it
2. Find the message input field and type "Hello from Alice!"
3. Press Enter to send the message
4. Verify "Hello from Alice!" appears in the chat area
5. Verify the online/user list shows "Alice"

### 2. Typing Indicators
**Seed:** `specs/seed.spec.ts`

#### 2.1 Typing Indicator Appears
**Steps:**
1. Find the name input and type "Alice", then submit
2. Find the room creation input, create a room called "TypingTest"
3. Click on "TypingTest" to enter it
4. Find the message input field
5. Type some text slowly without sending
6. Check if any text containing "typing" appears on the page

### 3. Read Receipts
**Seed:** `specs/seed.spec.ts`

#### 3.1 Seen Indicator Displays
**Steps:**
1. Find the name input and type "Alice", then submit
2. Create a room called "ReceiptTest"
3. Enter "ReceiptTest"
4. Send a message "Testing read receipts"
5. Verify the message appears
6. Look for any text containing "seen" or "read" near the messages

### 4. Unread Counts
**Seed:** `specs/seed.spec.ts`

#### 4.1 Unread Badge Shows
**Steps:**
1. Find the name input and type "Alice", then submit
2. Create two rooms: "Room1" and "Room2"
3. Enter "Room1" and send a message "Test message"
4. Look at the sidebar/room list for any numeric badge or unread indicator

### 5. Scheduled Messages
**Seed:** `specs/seed.spec.ts`

#### 5.1 Schedule Message UI
**Steps:**
1. Find the name input and type "Alice", then submit
2. Create a room called "ScheduleTest" and enter it
3. Look for a schedule button near the message input (may have clock icon, "Schedule" text)
4. If found, click it
5. Look for a time/date picker or duration input
6. Verify scheduling UI elements are present

### 6. Ephemeral Messages
**Seed:** `specs/seed.spec.ts`

#### 6.1 Disappearing Message UI
**Steps:**
1. Find the name input and type "Alice", then submit
2. Create a room called "EphemeralTest" and enter it
3. Look for an ephemeral/disappearing toggle (may be labeled "Ephemeral", "Disappearing", "Expire")
4. If found, interact with it to set a duration
5. Send a message with the ephemeral option enabled
6. Look for a countdown or expiry indicator on the message

### 7. Message Reactions
**Seed:** `specs/seed.spec.ts`

#### 7.1 Add Reaction to Message
**Steps:**
1. Find the name input and type "Alice", then submit
2. Create a room called "ReactionTest" and enter it
3. Send a message "React to this!"
4. Hover over the message to reveal action buttons
5. Look for a reaction button (may have emoji like 👍 or text "React")
6. Click it and select an emoji
7. Verify a reaction count appears on the message

### 8. Message Editing
**Seed:** `specs/seed.spec.ts`

#### 8.1 Edit Own Message
**Steps:**
1. Find the name input and type "Alice", then submit
2. Create a room called "EditTest" and enter it
3. Send a message "Original message"
4. Hover over the message to reveal action buttons
5. Look for an "Edit" button and click it
6. Change the text to "Edited message" and save
7. Verify the message now shows "Edited message"
8. Look for an "(edited)" indicator on the message

### 9. Permissions
**Seed:** `specs/seed.spec.ts`

#### 9.1 Admin Controls Visible
**Steps:**
1. Find the name input and type "Alice", then submit
2. Create a room called "AdminTest" and enter it
3. Look for admin-related buttons in the room header or member list (e.g., "Members", "Manage", "Admin")
4. Check if kick/promote buttons are visible when viewing member list

### 10. Rich User Presence
**Seed:** `specs/seed.spec.ts`

#### 10.1 Status Selector
**Steps:**
1. Find the name input and type "Alice", then submit
2. Look for a status selector (dropdown, select, or buttons with "Online", "Away", "DND")
3. If found, change the status to "Away"
4. Verify the status indicator changes (colored dot, text)
5. Change back to "Online"
6. Verify the indicator updates
