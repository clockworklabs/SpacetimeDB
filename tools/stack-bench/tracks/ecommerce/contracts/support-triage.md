## Support triage controls

Open the staff area with `staff-link`. If its ticket queue is on a separate tab or
screen, expose `support-queue-link` there to open it. Omit this control when the
editable ticket controls are already shown.

Use `support-ticket` for each ticket in the staff view. Within a ticket, use
`support-assignee`, `support-priority`, and `support-status-input` for the editable fields.
Use `support-update` to apply the changes. Use `support-status` to show the current status.
`support-assignee` takes the assignee's username; if it is a select, its option values are the
usernames. `support-priority` offers `low`, `normal`, and `high` as its option values.

`support-status-input` offers the statuses `open`, `in progress`, and `resolved` as its option
values; `support-status` shows the one chosen.
