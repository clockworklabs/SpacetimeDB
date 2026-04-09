# Feature 15: Anonymous to Registered Migration

**Max Score: 3** | **Multi-user: Single tab OK**

## Preconditions
- App supports anonymous usage (join without account)

## Test Steps

### Step 1: Anonymous Usage
1. Open a fresh tab (Tab C) or use incognito mode.
2. Navigate to the app. Look for a way to use it without registering — "Join as guest", "Skip registration", "Anonymous", or it may just work without a name.
3. Use `find("guest")` or `find("anonymous")` or `find("skip")`.
4. If the app requires a name, enter "AnonUser" or similar.
5. Join a room and send 3 messages: "anon msg 1", "anon msg 2", "anon msg 3".

**Criterion:** Users can join and send messages without an account (1 point)

### Step 2: Session Persistence
1. Refresh the page (use `javascript_tool` to run `window.location.reload()`).
2. **Verify**: The anonymous identity persists — still recognized as the same user, still in the same room.
3. Use `get_page_text` to verify the username is still visible and messages are attributed correctly.

**Criterion:** Anonymous identity persists for the session (0.5 points)

### Step 3: Registration Migration
1. Find a "Register" or "Create Account" or "Sign Up" button. Use `find("register")` or `find("sign up")` or `find("create account")`.
2. Register with a proper username and any required credentials.
3. **Verify**: After registration, the 3 anonymous messages are still attributed to this user. Use `get_page_text` to find "anon msg 1" etc. and check the author name matches the new registered name.

**Criterion:** Registration preserves message history and identity (1 point)

### Step 4: Room Membership Transfer
1. **Verify**: The user is still a member of the room they joined anonymously.
2. Other users in the room should not see a "user left/joined" event — the transition should be seamless.

**Criterion:** Room memberships transfer to registered account (0.5 points)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met, seamless migration |
| 2 | Can register but some history is lost |
| 1 | Anonymous works but registration creates new identity |
| 0 | Not implemented |

## Notes
- This feature depends heavily on how the app implements authentication. If the app requires registration upfront, this entire feature scores 0.
- The "seamless" aspect is key — no data loss, no disruption for other users.

## Evidence
- Screenshot of anonymous user's messages before registration
- Screenshot of same messages after registration (attributed to new username)
