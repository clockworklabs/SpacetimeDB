## Support intake controls

Use `support-link` to open support. Show it directly while signed out. While signed in,
show it directly or reveal it by clicking `current-user`. No other navigation is required
to reach it. Opening support makes the intake form available without another tab or menu.
If a support dialog, panel, or confirmation overlay blocks leaving support, account
navigation, or the intake form, expose a visible `overlay-close` control that dismisses
the blocking overlay. Screens without a blocking overlay need no such control.
Dialogs, panels, and ordinary page layouts are all allowed.
Use `support-email`, `support-subject`, and
`support-message` for the ticket fields. Use `support-submit` to submit the ticket and
`support-reference` to show its reference.

An email address, a subject, and a message are sufficient to submit a ticket. Any
additional fields must be optional.
