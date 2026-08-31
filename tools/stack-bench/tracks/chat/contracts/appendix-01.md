

## Application interface

Use these exact `id` attributes on the corresponding visible controls. They do
not prescribe the design, architecture, or implementation.

Rules:
- Attribute name is exactly `id`; values are exactly as listed (kebab-case).
- Repeated elements such as rooms and messages use the listed ID on every instance.
- An element that is hidden until a menu/toggle opens still counts, as long as it is in the DOM after its toggle is clicked.
- Do not reuse these IDs on unrelated elements.

| Element ID | Element |
|---|---|
| `app-title` | the app's visible title, naming which backend it is built on (for example "PostgreSQL Chat") |
| `signup-username` | username input on the sign-up form |
| `signup-password` | password input on the sign-up form |
| `signup-submit` | button that creates the account |
| `signin-toggle` | control that switches from the sign-up form to the sign-in form |
| `signin-username` | username input on the sign-in form |
| `signin-password` | password input on the sign-in form |
| `signin-submit` | button that signs the user in with existing credentials |
| `auth-error` | visible after a rejected sign-up or sign-in; must not appear on success |
| `current-user` | element displaying the signed-in account's username |
| `signout` | button that signs the current user out |
| `room-create` | control that starts creating a new room |
| `room-name-input` | text input for the new room's name |
| `room-name-submit` | button that confirms room creation |
| `room-list` | container listing the rooms |
| `room-item` | one entry per room inside the room list, containing the room name |
| `online-users` | container listing the usernames of users currently online |
| `online-user` | appears while that user is signed in, gone once they sign out |
| `message-input` | text input for composing a message; pressing Enter sends it |
| `message-list` | scrollable container holding the room's messages |
| `message-item` | one entry per message, containing the message text and its author's username |
| `leave-room` | button that leaves the current room |
| `send-error` | visible after a rejected send; must not appear on a successful send |
| `typing-indicator` | visible while another user in the SAME room is typing; gone within 6s of inactivity |
| `read-receipt` | appears under a message after another user has viewed it |
| `unread-badge` | appears on a room entry that has unread messages for the current user; clears when that room is opened |
| `react-button` | present on each message; clicking it adds the current user's reaction |
| `reaction-count` | shows how many users reacted with that emoji; repeated per emoji per message |
| `pin-button` | present on each message |
| `pinned-item` | the room's pinned messages, capped at three |
| `pin-error` | visible after a rejected pin; must not appear on a successful one |
| `pinned-unpin` | present on each pinned-item |
| `member-panel` | container listing the members of the current room |
| `member-item` | appears for each member of the current room; gone once they leave or are removed |
| `member-owner` | present on the owner's entry only |
| `member-remove` | shown to owners only |
| `member-promote` | shown to owners only |
| `dm-start` | present on each other user listed as online, and on each member entry |
| `dm-list` | container listing the user's direct conversations |
| `dm-item` | appears once a direct conversation exists; visible only to its two participants |
| `permission-error` | visible after a refused unpin, removal, promotion, edit or delete |
| `message-edit` | shown on the author's own messages |
| `message-delete` | shown on the author's own messages |
| `edited-marker` | appears on a message after it is edited |

Before declaring DEPLOY_COMPLETE, verify the interface by running the contract
linter (command provided in your build instructions) and fix any failures.
