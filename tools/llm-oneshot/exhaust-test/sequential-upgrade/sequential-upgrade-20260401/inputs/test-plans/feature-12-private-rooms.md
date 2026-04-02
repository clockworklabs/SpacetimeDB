# Feature 12: Private Rooms and Direct Messages

**Max Score: 3** | **Multi-user: Yes**

## Preconditions
- Both users registered
- At least one public room exists

## Test Steps

### Step 1: Create Private Room
1. **Tab A**: Find the create room option. Look for a "private" checkbox, toggle, or room type selector. Use `find("private")` or `find("invite only")`.
2. Create a room called "Secret Room" and mark it as private.
3. **Switch to Tab B**: Check the room list. "Secret Room" should NOT appear. Use `get_page_text` to confirm absence.

**Criterion:** Users can create private/invite-only rooms (0.75 points)

### Step 2: Invite User
1. **Tab A**: In the private room, look for an "invite" option. Use `find("invite")`.
2. Invite Bob by username.
3. **Tab B**: Check for an invitation notification, or if Bob can now see "Secret Room" in the room list.
4. If there's an accept/decline flow, accept the invitation.
5. **Verify Tab B**: Bob can now see and access "Secret Room".

**Criterion:** Room creators can invite specific users by username (0.75 points)

### Step 3: Direct Messages
1. **Tab A**: Look for a "DM" or "direct message" option. Use `find("DM")` or `find("direct message")` or `find("message user")`.
2. Start a DM with Bob.
3. Send a DM message "Private hello!".
4. **Tab B**: Verify the DM conversation appears and contains "Private hello!".

**Criterion:** Direct messages between two users work (0.75 points)

### Step 4: Privacy Enforcement
1. Open a **Tab C** with user "Charlie" (or check from Tab B before joining the private room).
2. Verify Charlie cannot see "Secret Room" in the room list.
3. Verify Charlie cannot see the private room's messages or member list.

**Criterion:** Only members can see private room content (0.75 points)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met |
| 2 | Private rooms work but DMs missing or invites broken |
| 1 | Can mark rooms as private but visibility not enforced |
| 0 | Not implemented |

## Evidence
- Screenshot showing private room NOT in public list
- Screenshot of DM conversation
