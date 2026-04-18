package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * Marks declarations that are internal to the SpacetimeDB SDK and generated code.
 * Using them from user code is unsupported and may break without notice.
 */
@RequiresOptIn(
    message = "This is internal to the SpacetimeDB SDK and generated code. Do not use directly.",
    level = RequiresOptIn.Level.ERROR,
)
@Retention(AnnotationRetention.BINARY)
@Target(AnnotationTarget.CLASS, AnnotationTarget.CONSTRUCTOR, AnnotationTarget.FUNCTION, AnnotationTarget.PROPERTY)
public annotation class InternalSpacetimeApi
