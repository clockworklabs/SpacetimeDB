package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * A sum type representing either a successful [Ok] value or an [Err] error.
 * Corresponds to `Result<T, E>` in the SpacetimeDB module schema.
 */
public sealed interface SpacetimeResult<out Ok, out Err> {
    /** Successful variant holding [value]. */
    public data class Ok<out T>(val value: T) : SpacetimeResult<T, Nothing>
    /** Error variant holding [error]. */
    public data class Err<out E>(val error: E) : SpacetimeResult<Nothing, E>
}
