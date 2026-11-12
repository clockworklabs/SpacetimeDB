# Level 3 — Contended state

Adds shared state that many users mutate at the same time, on top of levels 1 and 2.
Everything from earlier levels must keep working.

## Features

### Reactions

- A user can add an emoji reaction to a message, and remove their own
- Each message shows each emoji with a **count** of how many users reacted with it
- A user may react at most once per emoji per message — reacting twice does not raise the
  count by two
- Counts update live for everyone in the room

### Polls

- A user can post a **poll** to a room: a question and two or more options
- Each user may cast **one vote**, and may change it — changing a vote moves the tally, it
  does not add to it
- The poll shows per-option vote counts and a total, updating live
- A poll can be closed by its author; votes after closing are rejected

### Limited-capacity events

- A user can create an **event** in a room with a fixed number of seats
- Users claim seats until the event is full; further claims are rejected with a visible error
- The number of claimed seats must never exceed the capacity, no matter how many users claim
  at the same moment
- A user can release a seat, freeing it for someone else
- Seat counts update live

Correctness under simultaneous use is the point of this level. Totals must be exact when
many users act at the same instant: no lost updates, no double-counting, no overselling.
