# `sdk-test-procedure-concurrency` *Rust* test

This module isolates procedure concurrency behavior that currently only has
Rust module coverage.

It is separate from [`sdk-test-procedure`](../sdk-test-procedure) so the shared
procedure test suite can continue targeting other module languages without also
requiring `ctx.sleep_until` support.
