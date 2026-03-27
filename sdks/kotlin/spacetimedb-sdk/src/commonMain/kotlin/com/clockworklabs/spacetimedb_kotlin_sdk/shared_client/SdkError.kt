package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * Marker interface for typed SDK errors.
 * All error types returned by SDK operations implement this interface,
 * enabling exhaustive `when` blocks on [SdkResult.Failure].
 */
public interface SdkError
