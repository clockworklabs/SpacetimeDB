# Description of Changes

<!-- What changes for users or maintainers, and why? Link related public issues where useful. -->

# API and ABI breaking changes

<!-- If this is an API or ABI breaking change, please apply the
corresponding GitHub label. -->

# Rollback safety impact

<!--
If this PR is safe to roll back after deployment and has no impact on the rollback safety of any prior PRs, write `n/a` below.

Otherwise, list every prerequisite PR that must be included in a release before this PR can merge. This PR must not be deployed until each prerequisite has been deployed and sufficiently observed or tested in the live deployment that we are confident it will not be rolled back. Supported forms include #123, SpacetimeDB#123, clockworklabs/SpacetimeDB#123, and GitHub PR URLs.

Consider whether this PR:
- newly writes to a previously introduced but unused ControlDB table or reducer, system table, or on-disk data format; or
- clears, deletes, incompatibly changes, or renders unsupported a previously available ControlDB table or reducer, system table, or on-disk data format.
-->

# Expected complexity level and risk

<!--
How complicated do you think these changes are? Grade on a scale from 1 to 5,
where 1 is a trivial change, and 5 is a deep-reaching and complex change.

This complexity rating applies not only to the complexity apparent in the diff,
but also to its interactions with existing and future code.

If you answered more than a 2, explain what is complex about the PR,
and what other components it interacts with in potentially concerning ways.  -->

# Testing

<!-- If untested, write only “Not tested.”
Otherwise, briefly name the behaviors covered and any important failure or gap.
Include a reviewer task only when a specific action is needed. -->
