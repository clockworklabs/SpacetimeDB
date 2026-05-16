package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/** Errors from a one-off SQL query. */
public sealed interface QueryError : SdkError {
    /** The server rejected the query or returned an error. */
    public data class ServerError(val message: String) : QueryError
    /** The connection was closed before the query result was received. */
    public data object Disconnected : QueryError
}

/** Errors from a procedure call. */
public sealed interface ProcedureError : SdkError {
    /** The server reported an internal error executing the procedure. */
    public data class InternalError(val message: String) : ProcedureError
    /** The connection was closed before the procedure result was received. */
    public data object Disconnected : ProcedureError
}

/** Errors from a subscription. */
public sealed interface SubscriptionError : SdkError {
    /** The server rejected the subscription query. */
    public data class ServerError(val message: String) : SubscriptionError
}
