# Feature 2: Typing Indicators

**Max Score: 3** | **Multi-user: Yes + Timing**

## Preconditions
- Both users (Alice in Tab A, Bob in Tab B) are in the same room
- Messages have been sent successfully (Feature 1 passes)

## Important: Triggering Typing Events

`form_input` and `computer(type)` may NOT trigger React's `onChange` handler, which means the app won't fire the typing reducer. You MUST use `javascript_tool` to trigger typing with React-compatible synthetic events:

```javascript
// Run this in the tab where you want to trigger typing
const input = document.querySelector('input[placeholder*="message" i], input[placeholder*="type" i], textarea');
if (input) {
  const nativeInputValueSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  nativeInputValueSetter.call(input, 'typing test...');
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
}
```

If the app uses a `textarea` instead of `input`, adjust the selector and use `HTMLTextAreaElement.prototype` instead.

## Important: Verifying Timing-Sensitive UI

Typing indicators appear and disappear within seconds. **Do NOT rely on screenshots** for verification — they are too slow. Use `get_page_text` which returns immediately and search for "typing" in the result. Screenshots are only for supplementary evidence.

## Test Steps

### Step 1: Typing Broadcast
1. **Tab B (Bob)**: Use `javascript_tool` to trigger a typing event (see snippet above).
2. **Tab A (Alice)**: IMMEDIATELY call `get_page_text` and search for "typing" or "is typing".
3. **Verify**: The text should contain "Bob is typing..." or similar.

**Criterion:** Typing state is broadcast to other room members (1 point)

### Step 2: Auto-Expiry
1. **Do NOT trigger any more typing events.**
2. Wait 6 seconds: `computer(action: "wait", duration: 6)`.
3. **Tab A (Alice)**: Call `get_page_text` and search for "typing".
4. **Verify**: The "is typing" text should be GONE.

**Criterion:** Typing indicator auto-expires after inactivity (1 point)

### Step 3: UI Display & Multiple Users
1. **Tab B (Bob)**: Trigger typing via `javascript_tool`.
2. **Tab A (Alice)**: Also trigger typing via `javascript_tool`.
3. **Tab B (Bob)**: Call `get_page_text` — check for "Alice is typing..." or "Multiple users are typing...".
4. **Tab A (Alice)**: Call `get_page_text` — check for "Bob is typing...".

**Criterion:** UI shows appropriate typing message for each user (1 point)

### Step 4: Clear on Send (Bonus verification)
1. **Tab B (Bob)**: Trigger typing, then immediately send a message (submit the form).
2. **Tab A (Alice)**: Call `get_page_text` — the typing indicator should clear immediately (not wait for timeout).

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met, updates in real-time, auto-expiry works |
| 2 | Works but noticeable delay, or missing multi-user display, or expiry doesn't work |
| 1 | Typing tracked but doesn't display on other user's screen, or never expires |
| 0 | Not implemented or no typing reducer exists |

## Timing Notes
- The auto-expiry test (Step 2) requires a 6-second wait. This is the minimum.
- Between triggering typing and checking the other tab, act FAST — use `get_page_text`, not screenshots.
- If the indicator is already gone by the time you check, retry: trigger typing and check within 1-2 seconds.

## Evidence
- `get_page_text` output showing "is typing" text (primary evidence)
- `get_page_text` output after timeout showing "is typing" is gone
- Optional: screenshot if you can capture it fast enough
