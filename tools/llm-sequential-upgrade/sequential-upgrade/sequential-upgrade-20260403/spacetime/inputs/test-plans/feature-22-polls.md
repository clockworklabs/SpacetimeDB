# Feature 22: Polls

**Max Score: 3** | **Multi-user: Yes (2 tabs)**

## Preconditions
- Both users (Alice in Tab A, Bob in Tab B) are in the same room

## Test Steps

### Step 1: Create a Poll
1. **Tab A (Alice)**: Look for a poll creation button (poll icon, "Create Poll", or similar).
2. Create a poll with question "Favorite color?" and options "Red", "Blue", "Green".
3. **Verify Tab A**: Poll appears in the channel with the question and all options showing 0 votes.
4. **Verify Tab B**: Bob also sees the poll in real-time.

**Criterion:** Users can create polls with question and options, visible to all in real-time (1 point)

### Step 2: Vote and Real-Time Updates
1. **Tab A (Alice)**: Vote for "Blue".
2. **Verify Tab A**: "Blue" shows 1 vote.
3. **Verify Tab B**: Bob also sees "Blue" with 1 vote in real-time.
4. **Tab B (Bob)**: Vote for "Red".
5. **Verify**: Both tabs show Blue=1, Red=1 in real-time.
6. **Tab A (Alice)**: Try voting again for "Green" (changing vote).
7. **Verify**: Blue drops to 0, Green shows 1. No double counting.

**Criterion:** Votes update in real-time, changing vote removes previous vote atomically (1 point)

### Step 3: Close Poll and Voter Visibility
1. **Tab A (Alice)**: Close the poll (as creator).
2. **Verify Tab B**: Bob can no longer vote — UI indicates poll is closed.
3. Hover over or expand vote counts to see voter names.
4. **Verify**: Voter names are visible (e.g., "Alice voted Green", "Bob voted Red").

**Criterion:** Poll creator can close poll, voter names visible (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria pass — create, vote with real-time sync, change vote atomically, close, voter names |
| 2 | Voting works but missing close or voter visibility |
| 1 | Poll created but voting broken or no real-time sync |
| 0 | Not implemented |
