# Level 4 — Deferred and expiring work

Adds work that happens later, on top of levels 1 to 3. Everything from earlier levels must
keep working.

## Features

### Scheduled messages

- A user can compose a message and schedule it to send at a future time, as little as 60
  seconds ahead
- The author sees their pending scheduled messages and can cancel one before it sends
- A scheduled message does not appear in the room before its time
- At its time it appears for everyone in the room, attributed to its author
- **Pending scheduled messages survive a backend restart** and still send at their time

### Expiring messages

- A user can send a message that expires after a chosen duration, selectable down to 30
  seconds
- An expiring message shows a countdown or expiry indicator
- When it expires it disappears for everyone, live, without a reload
- Expired content is **actually deleted** from storage, not merely hidden in the interface
- Expiry survives a backend restart: a message due to expire during the restart is gone
  afterwards, and one not yet due still expires on time

### Reminders

- A user can set a personal reminder on a message for a future time
- At that time the user is notified in the app
- A reminder is visible only to the user who set it, and survives a restart
