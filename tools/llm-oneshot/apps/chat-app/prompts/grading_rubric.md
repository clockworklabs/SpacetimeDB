# Chat App Benchmark Grading Rubric

Use this rubric to evaluate LLM-generated chat applications for both SpacetimeDB and PostgreSQL implementations.

## Prompt-to-Feature Mapping

**Only score features that were included in the prompt used.** Each composed prompt level includes all features up to that number:

| Prompt Level         | Features Included                          | Max Score |
| -------------------- | ------------------------------------------ | --------- |
| `01_*_basic`         | 1-4 (Basic, Typing, Read Receipts, Unread) | 12        |
| `02_*_scheduled`     | 1-5 (+ Scheduled Messages)                 | 15        |
| `03_*_realtime`      | 1-6 (+ Ephemeral Messages)                 | 18        |
| `04_*_reactions`     | 1-7 (+ Reactions)                          | 21        |
| `05_*_edit_history`  | 1-8 (+ Edit History)                       | 24        |
| `06_*_permissions`   | 1-9 (+ Permissions)                        | 27        |
| `07_*_presence`      | 1-10 (+ Rich Presence)                     | 30        |
| `08_*_threading`     | 1-11 (+ Threading)                         | 33        |
| `09_*_private_rooms` | 1-12 (+ Private Rooms & DMs)               | 36        |
| `10_*_activity`      | 1-13 (+ Activity Indicators)               | 39        |
| `11_*_drafts`        | 1-14 (+ Draft Sync)                        | 42        |
| `12_*_anonymous`     | 1-15 (All features)                        | 45        |

**Example:** If you used `05_spacetime_edit_history.md`, only score features 1-8 and use 24 as the max score.

---

## Scoring System

Each feature is scored on a **0–3 scale**:

| Score   | Meaning                                                           |
| ------- | ----------------------------------------------------------------- |
| **0**   | Not implemented or completely broken                              |
| **1**   | Partially implemented; major issues or missing core functionality |
| **2**   | Mostly working; minor bugs or missing edge cases                  |
| **3**   | Fully working as specified                                        |
| **N/A** | Feature not included in prompt (don't count toward total)         |

---

## Overall Metrics (Record These)

| Metric                        | Value                                              |
| ----------------------------- | -------------------------------------------------- |
| **Prompt Level Used**         | \_\_\_ (e.g., `05_spacetime_edit_history`)         |
| **Features Evaluated**        | 1-\_\_\_ (e.g., 1-8)                               |
| **Total Feature Score**       | **_ / _** (features × 3 points)                    |
| **Compiles without errors**   | Yes / No                                           |
| **Runs without crashing**     | Yes / No                                           |
| **Lines of code (backend)**   | \_\_\_                                             |
| **Lines of code (frontend)**  | \_\_\_                                             |
| **Number of files created**   | \_\_\_                                             |
| **External dependencies**     | List them                                          |
| **First-try success**         | Yes / No (did it work without manual fixes?)       |
| **Reprompt Count**            | \_\_\_ (number of follow-up prompts to fix issues) |
| **Reprompt Efficiency Score** | \_\_\_ / 10 (see Reprompt Scoring below)           |

---

## Reprompt Scoring

Track how many follow-up prompts are needed to get the application working. This measures how close the LLM gets to a working solution on the first attempt.

### What Counts as a Reprompt

A reprompt is any follow-up message you send to fix issues, including:

- "Fix the compilation error in X"
- "The app crashes when I do Y, please fix"
- "Feature Z isn't working as expected"
- "The server won't start because..."
- Pasting error messages and asking for fixes

**Does NOT count as a reprompt:**

- Asking clarifying questions before the LLM starts coding
- Requesting additional features not in the original prompt
- Asking for explanations of how code works
- Normal conversation that doesn't involve fixing bugs

### Reprompt Categories

| Category              | Description                                          |
| --------------------- | ---------------------------------------------------- |
| **Compilation/Build** | Code doesn't compile, missing imports, syntax errors |
| **Runtime/Crash**     | App crashes on startup or during use                 |
| **Feature Broken**    | Feature exists but doesn't work correctly            |
| **Integration**       | Frontend/backend don't communicate properly          |
| **Data/State**        | Data not persisting, state management issues         |

### Reprompt Efficiency Score

| Reprompts | Score | Interpretation                         |
| --------- | ----- | -------------------------------------- |
| 0         | 10    | Perfect - works on first try           |
| 1         | 9     | Excellent - minor fix needed           |
| 2         | 8     | Very Good - few issues                 |
| 3         | 7     | Good - some debugging required         |
| 4-5       | 6     | Acceptable - moderate iteration        |
| 6-7       | 5     | Below Average - significant debugging  |
| 8-10      | 4     | Poor - extensive iteration required    |
| 11-15     | 2     | Very Poor - major rework needed        |
| 16+       | 0     | Failing - essentially pair programming |

### Reprompt Tracking Template

| #   | Category | Issue Summary | Fixed? |
| --- | -------- | ------------- | ------ |
| 1   |          |               | Yes/No |
| 2   |          |               | Yes/No |
| 3   |          |               | Yes/No |
| 4   |          |               | Yes/No |
| 5   |          |               | Yes/No |
| ... |          |               |        |

**Total Reprompts:** **_  
**Categories Breakdown:** Compilation: _** | Runtime: **_ | Feature: _** | Integration: **_ | Data: _**

---

## Feature 1: Basic Chat Features

**Max Score: 3**

| Criteria                                                       | Points |
| -------------------------------------------------------------- | ------ |
| Users can set a display name                                   | 0.5    |
| Users can create chat rooms                                    | 0.5    |
| Users can join/leave rooms                                     | 0.5    |
| Users can send messages to joined rooms                        | 0.5    |
| Online users are displayed                                     | 0.5    |
| Basic validation exists (no empty messages, name limits, etc.) | 0.5    |

**Scoring:**

- 3: All criteria met
- 2: 4-5 criteria met
- 1: 2-3 criteria met
- 0: 0-1 criteria met

**Test Cases:**

1. Create user with name → name displays correctly
2. Create room → room appears in list
3. Join room → user appears in room members
4. Send message → message appears for all users in room
5. Leave room → user removed from members, stops seeing messages
6. Try to send empty message → rejected
7. Try to send message to room not joined → rejected

---

## Feature 2: Typing Indicators

**Max Score: 3**

| Criteria                                                       | Points |
| -------------------------------------------------------------- | ------ |
| Typing state is broadcast to other room members                | 1      |
| Typing indicator auto-expires after inactivity (3-5 seconds)   | 1      |
| UI shows "User is typing..." or "Multiple users are typing..." | 1      |

**Scoring:**

- 3: All criteria met, updates in real-time
- 2: Works but with noticeable delay or missing multi-user display
- 1: Typing is tracked but doesn't expire or UI is broken
- 0: Not implemented

**Test Cases:**

1. User A types in room → User B sees "A is typing..."
2. User A stops typing → indicator disappears after timeout
3. User A and B both type → shows "Multiple users are typing..."
4. User A sends message → typing indicator clears immediately

---

## Feature 3: Read Receipts

**Max Score: 3**

| Criteria                                                       | Points |
| -------------------------------------------------------------- | ------ |
| System tracks which users have seen which messages             | 1      |
| "Seen by X, Y, Z" or similar indicator displays under messages | 1      |
| Read status updates in real-time as users view messages        | 1      |

**Scoring:**

- 3: All criteria met, real-time updates
- 2: Works but laggy or shows only "seen" without names
- 1: Read state is tracked but not displayed properly
- 0: Not implemented

**Test Cases:**

1. User A sends message → shows as unread/not seen
2. User B opens the room → message shows "Seen by B"
3. User C opens the room → updates to "Seen by B, C" in real-time
4. User A sees their own messages marked as seen by others

---

## Feature 4: Unread Message Counts

**Max Score: 3**

| Criteria                                                        | Points |
| --------------------------------------------------------------- | ------ |
| Unread count badge shows on room list                           | 1      |
| Count tracks last-read position per user per room               | 1      |
| Counts update in real-time (new messages arrive, messages read) | 1      |

**Scoring:**

- 3: All criteria met
- 2: Counts work but don't update in real-time (need refresh)
- 1: Badge shows but count is incorrect
- 0: Not implemented

**Test Cases:**

1. User A is in Room 1, User B sends message to Room 2 → Room 2 shows "(1)" badge for A
2. Three more messages sent → badge shows "(4)"
3. User A opens Room 2 → badge clears
4. User A switches to Room 1, new message in Room 2 → badge shows "(1)" in real-time

---

## Feature 5: Scheduled Messages

**Max Score: 3**

| Criteria                                                        | Points |
| --------------------------------------------------------------- | ------ |
| Users can compose and schedule messages for future delivery     | 1      |
| Pending scheduled messages visible to author with cancel option | 1      |
| Message appears in room at scheduled time                       | 1      |

**Scoring:**

- 3: All criteria met, timing is accurate (within a few seconds)
- 2: Works but timing is off or cancel doesn't work
- 1: Can schedule but messages never appear or appear immediately
- 0: Not implemented

**Test Cases:**

1. Schedule message for 1 minute from now → message doesn't appear yet
2. Author sees pending message in UI with cancel option
3. Author cancels → message never appears
4. Schedule another message → appears at correct time
5. Other users see the message at the scheduled time (not before)

---

## Feature 6: Ephemeral/Disappearing Messages

**Max Score: 3**

| Criteria                                          | Points |
| ------------------------------------------------- | ------ |
| Users can send messages with auto-delete timer    | 1      |
| Countdown or disappearing indicator shown in UI   | 1      |
| Message is permanently deleted when timer expires | 1      |

**Scoring:**

- 3: All criteria met, deletion happens on schedule
- 2: Messages delete but no visual countdown, or timing is inaccurate
- 1: Option exists but messages don't actually delete
- 0: Not implemented

**Test Cases:**

1. Send ephemeral message with 30-second timer → shows countdown
2. Wait 30 seconds → message disappears for all users
3. Check database/storage → message is actually deleted (not just hidden)
4. Ephemeral messages in threads also work correctly

---

## Feature 7: Message Reactions

**Max Score: 3**

| Criteria                                        | Points |
| ----------------------------------------------- | ------ |
| Users can add emoji reactions to messages       | 0.75   |
| Reaction counts display and update in real-time | 0.75   |
| Users can toggle their own reactions on/off     | 0.75   |
| Hover/click shows who reacted                   | 0.75   |

**Scoring:**

- 3: All criteria met
- 2: Reactions work but missing hover details or toggle is buggy
- 1: Can react but counts don't update in real-time
- 0: Not implemented

**Test Cases:**

1. User A adds 👍 to message → shows "👍 1"
2. User B adds 👍 → updates to "👍 2" in real-time
3. User A clicks 👍 again → removes their reaction, shows "👍 1"
4. Hover over reaction → shows "B reacted"
5. Multiple different reactions on same message work

---

## Feature 8: Message Editing with History

**Max Score: 3**

| Criteria                                      | Points |
| --------------------------------------------- | ------ |
| Users can edit their own messages             | 1      |
| "(edited)" indicator shows on edited messages | 0.5    |
| Edit history is viewable by other users       | 1      |
| Edits sync in real-time to all viewers        | 0.5    |

**Scoring:**

- 3: All criteria met
- 2: Editing works but history not viewable or no "(edited)" label
- 1: Can edit but changes don't sync in real-time
- 0: Not implemented

**Test Cases:**

1. User A sends message, then edits it → message updates for all users
2. Edited message shows "(edited)" indicator
3. Click on indicator or message → shows original and all versions
4. User A cannot edit User B's messages
5. Edit syncs in real-time without refresh

---

## Feature 9: Real-Time Permissions

**Max Score: 3**

| Criteria                                                        | Points |
| --------------------------------------------------------------- | ------ |
| Room creator is admin and can kick/ban users                    | 1      |
| Kicked users immediately lose access and stop receiving updates | 1      |
| Admins can promote other users to admin                         | 0.5    |
| Permission changes apply instantly (no reconnection needed)     | 0.5    |

**Scoring:**

- 3: All criteria met, instant enforcement
- 2: Works but requires refresh or reconnection
- 1: Admin can kick but kicked user still sees messages
- 0: Not implemented

**Test Cases:**

1. Room creator can access admin controls; others cannot
2. Admin kicks User B → User B is immediately removed from room
3. User B can no longer see new messages in that room
4. Admin promotes User C → User C can now kick others
5. Banned user cannot rejoin the room

---

## Feature 10: Rich User Presence

**Max Score: 3**

| Criteria                                                      | Points |
| ------------------------------------------------------------- | ------ |
| Users can set status: online, away, do-not-disturb, invisible | 1      |
| "Last active X minutes ago" shows for offline users           | 0.5    |
| Status changes sync to all viewers in real-time               | 1      |
| Auto-set to "away" after inactivity period                    | 0.5    |

**Scoring:**

- 3: All criteria met
- 2: Manual status works but no auto-away or last-active
- 1: Status exists but doesn't sync in real-time
- 0: Not implemented

**Test Cases:**

1. User sets status to "away" → others see away indicator
2. User sets "invisible" → appears offline to others but can still chat
3. User goes inactive → status auto-changes to "away"
4. User comes back → status auto-changes to "online"
5. Offline user shows "Last active 5 minutes ago"

---

## Feature 11: Message Threading

**Max Score: 3**

| Criteria                                                | Points |
| ------------------------------------------------------- | ------ |
| Users can reply to specific messages, creating a thread | 1      |
| Parent messages show reply count and preview            | 0.5    |
| Threaded view shows all replies to a message            | 1      |
| New replies sync in real-time to thread viewers         | 0.5    |

**Scoring:**

- 3: All criteria met
- 2: Threading works but no reply count or preview
- 1: Can reply but threaded view is broken
- 0: Not implemented

**Test Cases:**

1. Click "reply" on message → compose reply UI appears
2. Send reply → appears in thread, not main chat flow
3. Parent message shows "3 replies" badge
4. Click parent → opens threaded view with all replies
5. Another user adds reply → appears in real-time for all thread viewers

---

## Feature 12: Private Rooms and Direct Messages

**Max Score: 3**

| Criteria                                                   | Points |
| ---------------------------------------------------------- | ------ |
| Users can create private/invite-only rooms                 | 0.75   |
| Room creators can invite specific users by username        | 0.75   |
| Direct messages (DMs) between two users work               | 0.75   |
| Only members can see private room content and member lists | 0.75   |

**Scoring:**

- 3: All criteria met
- 2: Private rooms work but DMs missing or invites broken
- 1: Can mark rooms as private but visibility not enforced
- 0: Not implemented

**Test Cases:**

1. Create private room → does not appear in public room list
2. Invite User B by username → User B receives invitation
3. User B accepts → can see room and messages
4. User C (not invited) → cannot see room or its messages
5. Start DM with User D → private conversation between two users
6. DM appears in both users' room lists but no one else's

---

## Feature 13: Room Activity Indicators

**Max Score: 3**

| Criteria                                              | Points |
| ----------------------------------------------------- | ------ |
| Activity badges show on rooms (e.g., "Active", "Hot") | 1      |
| Activity level reflects recent message velocity       | 1      |
| Indicators update in real-time as activity changes    | 1      |

**Scoring:**

- 3: All criteria met
- 2: Shows activity but doesn't update in real-time
- 1: Static badge that doesn't reflect actual activity
- 0: Not implemented

**Test Cases:**

1. Room with messages in last 5 minutes shows "Active" indicator
2. Room with high message volume shows "Hot" or similar
3. Quiet room shows no badge or "Quiet" indicator
4. Send burst of messages → activity indicator updates in real-time
5. Activity indicator helps identify where conversations are happening

---

## Feature 14: Draft Sync

**Max Score: 3**

| Criteria                                         | Points |
| ------------------------------------------------ | ------ |
| Message drafts save automatically as user types  | 1      |
| Drafts sync across devices/sessions in real-time | 1      |
| Each room maintains its own draft per user       | 0.5    |
| Drafts persist until sent or manually cleared    | 0.5    |

**Scoring:**

- 3: All criteria met
- 2: Drafts save locally but don't sync across devices
- 1: Drafts exist but are lost on page refresh
- 0: Not implemented

**Test Cases:**

1. Start typing in Room A → switch to Room B → switch back → draft preserved
2. Open same account in new tab/device → draft appears
3. Update draft in Tab 1 → appears in Tab 2 in real-time
4. Send message → draft clears
5. Each room has independent draft

---

## Feature 15: Anonymous to Registered Migration

**Max Score: 3**

| Criteria                                            | Points |
| --------------------------------------------------- | ------ |
| Users can join and send messages without an account | 1      |
| Anonymous identity persists for the session         | 0.5    |
| Registration preserves message history and identity | 1      |
| Room memberships transfer to registered account     | 0.5    |

**Scoring:**

- 3: All criteria met, seamless migration
- 2: Can register but some history is lost
- 1: Anonymous works but registration creates new identity
- 0: Not implemented

**Test Cases:**

1. User joins without account → can chat with temporary name
2. Refresh page → still has same anonymous identity (session persistence)
3. Anonymous user sends 5 messages, joins 2 rooms
4. User registers → all 5 messages still attributed to them
5. User's room memberships preserved after registration
6. Other users see no disruption (no "user left/joined" spam)

---

## Summary Score Sheet

| Feature                  | Max    | Score | Notes |
| ------------------------ | ------ | ----- | ----- |
| 1. Basic Chat            | 3      |       |       |
| 2. Typing Indicators     | 3      |       |       |
| 3. Read Receipts         | 3      |       |       |
| 4. Unread Counts         | 3      |       |       |
| 5. Scheduled Messages    | 3      |       |       |
| 6. Ephemeral Messages    | 3      |       |       |
| 7. Message Reactions     | 3      |       |       |
| 8. Message Editing       | 3      |       |       |
| 9. Real-Time Permissions | 3      |       |       |
| 10. Rich Presence        | 3      |       |       |
| 11. Message Threading    | 3      |       |       |
| 12. Private Rooms & DMs  | 3      |       |       |
| 13. Activity Indicators  | 3      |       |       |
| 14. Draft Sync           | 3      |       |       |
| 15. Anonymous Migration  | 3      |       |       |
| **TOTAL**                | **45** |       |       |

---

## Comparison Template

| Metric              | SpacetimeDB | PostgreSQL |
| ------------------- | ----------- | ---------- |
| Total Score         | /45         | /45        |
| Compiles            | Yes/No      | Yes/No     |
| Runs                | Yes/No      | Yes/No     |
| Backend LOC         |             |            |
| Frontend LOC        |             |            |
| Total Files         |             |            |
| Dependencies        |             |            |
| First-try Success   | Yes/No      | Yes/No     |
| Reprompt Count      |             |            |
| Reprompt Efficiency | /10         | /10        |

### Combined Score (Optional)

To create a single comparable metric that weighs both feature completeness and iteration efficiency:

**Combined Score = (Feature Score / Max Score × 70) + (Reprompt Efficiency × 3)**

This weights feature completeness at 70% and iteration efficiency at 30%, for a max score of 100.

| Rating     | Combined Score |
| ---------- | -------------- |
| Excellent  | 90-100         |
| Good       | 75-89          |
| Acceptable | 60-74          |
| Poor       | 40-59          |
| Failing    | 0-39           |

---

## Notes

- **Test each feature independently** where possible
- **Document any manual fixes** required to get the app running
- **Screenshot or record** significant bugs for reference
- **Time how long** each implementation takes to evaluate (setup, testing)
- For partial scores, round to nearest 0.5
- **Track reprompts in real-time** as you debug - it's easy to lose count afterward
- **Copy error messages** when reprompting so you have a record of what went wrong
