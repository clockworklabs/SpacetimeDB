# Bug Report

## Bug 1: Sending a main-room message makes thread replies appear in the main chat

**Feature:** Message Threading

**Description:** Thread replies are shown correctly in the thread view, but when a new message
is sent in the main room, that thread's replies also render in the main room chat flow. This
includes replies whose parent was an ephemeral message that has already been destroyed — the
orphaned reply surfaces in the main chat on the next main-room message.

**Expected:** Thread replies never appear in the main room chat flow.

**Actual:** Sending a message in the main room causes thread replies (including replies to
already-destroyed ephemeral messages) to appear in the main room chat.
