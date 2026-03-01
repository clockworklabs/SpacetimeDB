package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * Sealed hierarchy for generated table handles.
 * Use `is RemotePersistentTable` / `is RemoteEventTable` to distinguish at runtime.
 *
 * - [RemotePersistentTable]: rows are stored in the client cache; supports
 *   count/all/iter, onDelete, onUpdate, onBeforeDelete, indexes, and remoteQuery.
 * - [RemoteEventTable]: rows are NOT stored; only onInsert fires per event.
 */
public sealed interface RemoteTable

/**
 * Marker for generated table handles backed by a persistent (stored) table.
 */
public interface RemotePersistentTable : RemoteTable

/**
 * Marker for generated table handles backed by an event (non-stored) table.
 */
public interface RemoteEventTable : RemoteTable
