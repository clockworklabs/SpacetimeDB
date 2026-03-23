package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

public sealed interface SpacetimeResult<out Ok, out Err> {
    public data class Ok<out T>(val value: T) : SpacetimeResult<T, Nothing>
    public data class Err<out E>(val error: E) : SpacetimeResult<Nothing, E>
}
