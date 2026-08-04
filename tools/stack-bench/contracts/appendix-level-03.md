

## Appendix: Testing Hooks (required)

The app is graded by an automated harness that locates elements **only** via
`data-testid` attributes. Add the exact test IDs below to the corresponding
elements. These are plain HTML attributes — they must not change your design,
styling, architecture, or backend in any way.

Rules:
- Attribute name is exactly `data-testid`; values are exactly as listed (kebab-case).
- Repeated elements (each room in the list, each message) carry the same testid on every instance.
- An element that is hidden until a menu/toggle opens still counts, as long as it is in the DOM after its toggle is clicked.
- Do not add testids beyond this list to elements that could be confused with these.

| Test ID | Element |
|---|---|
| `name-input` | text input where the user types their display name |
| `name-submit` | button that submits the display name and registers the user |
| `room-create` | control that starts creating a new room |
| `room-name-input` | text input for the new room's name |
| `room-name-submit` | button that confirms room creation (Enter in room-name-input must also work) |
| `room-list` | container listing the rooms; each room entry is clickable to enter it |
| `room-item` | one entry per room inside the room list (repeated testid), containing the room name as text |
| `online-users` | container listing online users' display names |
| `message-input` | text input for composing a message; pressing Enter sends it |
| `message-list` | scrollable container holding the room's messages |
| `message-item` | one entry per message inside the message list (repeated testid), containing the message text |
| `leave-room` | button that leaves the current room |
| `unread-badge` | appears on a room entry when that room has unread messages for the current user |
| `typing-indicator` | visible while another user in the SAME room is typing; gone within 6s of inactivity |
| `read-receipt` | appears under a message after another user has viewed it |
| `schedule-toggle` | control in the compose area that opens the message-scheduling UI |
| `schedule-time` | input for choosing the send time; must allow scheduling as little as 60 seconds ahead |
| `scheduled-item` | appears after the author schedules a message, until it sends or is cancelled |
| `schedule-cancel` | present on each scheduled-item |
| `ephemeral-toggle` | control in the compose area that switches the next message to ephemeral/disappearing |
| `ephemeral-duration` | duration chooser for ephemeral messages; MUST include an option of 30 seconds or less |
| `ephemeral-indicator` | visible on ephemeral messages until they are deleted; the message row must be gone from the DOM after expiry |

Before declaring DEPLOY_COMPLETE, verify the hooks by running the contract
linter (command provided in your build instructions) and fix any failures.
