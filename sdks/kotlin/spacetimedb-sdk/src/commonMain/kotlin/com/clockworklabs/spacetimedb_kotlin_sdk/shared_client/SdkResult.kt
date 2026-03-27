package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * A discriminated union representing either a successful value or a typed error.
 * Unlike `kotlin.Result`, the error type [E] is preserved at compile time,
 * enabling exhaustive pattern matching on error variants.
 */
public sealed interface SdkResult<out T, out E : SdkError> {
    /** Successful outcome holding [data]. */
    public data class Success<out T>(val data: T) : SdkResult<T, Nothing>
    /** Failed outcome holding a typed [error]. */
    public data class Failure<out E : SdkError>(val error: E) : SdkResult<Nothing, E>
}

/** Alias for operations that succeed with [Unit] or fail with [E]. */
public typealias EmptySdkResult<E> = SdkResult<Unit, E>

/** Runs [action] if this is [SdkResult.Success], returns `this` unchanged. */
public inline fun <T, E : SdkError> SdkResult<T, E>.onSuccess(
    action: (T) -> Unit,
): SdkResult<T, E> {
    if (this is SdkResult.Success) action(data)
    return this
}

/** Runs [action] if this is [SdkResult.Failure], returns `this` unchanged. */
public inline fun <T, E : SdkError> SdkResult<T, E>.onFailure(
    action: (E) -> Unit,
): SdkResult<T, E> {
    if (this is SdkResult.Failure) action(error)
    return this
}

/** Transforms the success value with [transform], preserving errors. */
public inline fun <T, E : SdkError, R> SdkResult<T, E>.map(
    transform: (T) -> R,
): SdkResult<R, E> = when (this) {
    is SdkResult.Success -> SdkResult.Success(transform(data))
    is SdkResult.Failure -> this
}

/** Returns the success value, or `null` if this is a failure. */
public fun <T, E : SdkError> SdkResult<T, E>.getOrNull(): T? =
    (this as? SdkResult.Success)?.data

/** Returns the error, or `null` if this is a success. */
public fun <T, E : SdkError> SdkResult<T, E>.errorOrNull(): E? =
    (this as? SdkResult.Failure)?.error

/** Discards the success value, preserving only the success/failure status. */
public fun <E : SdkError> SdkResult<*, E>.asEmptyResult(): EmptySdkResult<E> =
    map { }
