# Feature 17: User Profiles

**Max Score: 3** | **Multi-user: Yes (2 tabs)**

## Preconditions
- Both users (Alice in Tab A, Bob in Tab B) are registered and in the same room
- Messages have been sent by both users

## Test Steps

### Step 1: Edit Profile
1. **Tab A (Alice)**: Look for a profile edit button/link (settings icon, clicking own name, or a profile section).
2. Edit the bio/status message to "Hello, I'm Alice!"
3. **Verify**: Profile shows the updated bio.

**Criterion:** Users can edit their profile (bio/status message) (1 point)

### Step 2: Profile Card
1. **Tab B (Bob)**: Click on Alice's username in a message or the member list.
2. **Verify**: A profile card/popover appears showing Alice's display name and bio ("Hello, I'm Alice!").

**Criterion:** Clicking a username shows a profile card with user info (1 point)

### Step 3: Real-Time Profile Propagation
1. **Tab A (Alice)**: Change her display name to "Alice2" (via profile edit or name change).
2. **Verify Tab A**: All of Alice's messages now show "Alice2" as the sender.
3. **Verify Tab B**: Bob also sees all of Alice's messages re-attributed to "Alice2" in real-time — no page refresh needed.
4. Member list, online users, and any other display of Alice's name should also update.

**Criterion:** Profile changes propagate to all views across all users in real-time (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria pass — edit, card, real-time propagation |
| 2 | Edit and card work but propagation requires refresh |
| 1 | Edit works but no profile card or propagation |
| 0 | Not implemented |

## Evidence
- Screenshot of profile edit form
- Screenshot of profile card/popover
- Screenshot showing name change reflected in messages on both tabs
