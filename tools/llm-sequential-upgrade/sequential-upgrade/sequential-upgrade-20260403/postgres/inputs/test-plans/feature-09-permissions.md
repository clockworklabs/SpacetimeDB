# Feature 9: Real-Time Permissions

**Max Score: 3** | **Multi-user: Yes**

## Preconditions
- Alice (Tab A) is the room creator/admin
- Bob (Tab B) is a regular member of the room

## Test Steps

### Step 1: Admin Controls
1. **Tab A**: Look for admin controls in the room — kick button, ban option, user management. Use `find("kick")` or `find("admin")` or `find("manage")`.
2. **Verify**: Alice can see admin controls.
3. **Tab B**: Verify Bob does NOT see admin controls (or they're disabled).

**Criterion:** Room creator is admin and can kick/ban users (1 point)

### Step 2: Kick User
1. **Tab A**: Kick Bob from the room using the admin controls.
2. **Switch to Tab B**: Verify Bob is immediately removed — the room view should change (redirected to room list, error message, or access denied).
3. Use `get_page_text` to confirm Bob can no longer see the room's messages.

**Criterion:** Kicked users immediately lose access (1 point)

### Step 3: Promote Admin
1. First, have Bob rejoin the room (if allowed — create a new room if needed).
2. **Tab A**: Look for a "promote" or "make admin" option for Bob. Use `find("promote")` or `find("admin")`.
3. Promote Bob to admin.
4. **Tab B**: Verify Bob now has admin controls (can see kick/ban options).

**Criterion:** Admins can promote other users to admin (0.5 points)

### Step 4: Instant Enforcement
1. All permission changes from Steps 2-3 should have applied without Tab B needing to refresh the page.

**Criterion:** Permission changes apply instantly (0.5 points)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met, instant enforcement |
| 2 | Works but requires refresh or reconnection |
| 1 | Admin can kick but kicked user still sees messages |
| 0 | Not implemented |

## Evidence
- Screenshot of admin controls visible to Alice
- Screenshot of Bob's view after being kicked
