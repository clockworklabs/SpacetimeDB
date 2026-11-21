# Level 5 — Volume

The app from levels 1 to 4 must stay correct and responsive as data and usage grow.
No new user-facing features; this level is about behaviour at scale.

## Requirements

### Data volume

- A room with **50,000 messages** loads and remains usable
- A user in **200 rooms** sees their room list without noticeable delay
- Message history is paginated or virtualised rather than loaded in full
- Reading a room's recent messages must not get slower as its history grows

### Concurrency

- **100 concurrent signed-in clients** across a shared set of rooms remain correct:
  every message reaches every client in its room, exactly once, in the same order for all
- Sustained writes continue to be delivered without loss under continuous load

### Responsiveness

- A sent message reaches other clients in the same room in **under one second at the 95th
  percentile** under the load above
- The interface stays responsive while receiving continuous updates

### Efficiency

- Per-message work must not grow with the size of the room's history
- Queries backing the main views use indexes rather than scanning
