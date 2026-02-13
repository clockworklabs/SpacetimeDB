# DBCache

Utilities used to maintain a client side cache of database tables.  These classes store rows received from the server and provide helpers for fast lookups.

## Files

- `BTreeUniqueIndex.h` – B-tree implementation of `IUniqueIndex` supporting range queries.
- `ClientCache.h` – Owns `FTableCache` objects and applies insert/delete diffs sent over the network.
- `IUniqueIndex.h` – Interface that unique index implementations conform to.
- `RowEntry.h` – Wrapper storing a row value with a reference count used by overlapping subscriptions.
- `TableAppliedDiff.h` – Describes the inserts, deletes and updates detected when applying a diff.
- `TableCache.h` – In-memory representation of a table and its unique indices.
- `TableHandle.h` – Lightweight helper exposing read only access to a cached table.
- `UniqueConstraintHandle.h` – Helper that allows typed lookups against a unique constraint.
- `UniqueIndex.h` – Hash map based implementation of a unique index.
- `WithBsatn.h` – Pairs a row with its serialized BSATN bytes for diff processing.

# Client Cache / Table Cache System

This module implements a **Client-Side Table Cache** for storing and querying rows efficiently using **Unique Constraints** and **B-Tree Multi-Key Indices**.  
Designed for **Unreal Engine 5**, it supports **point queries**, **multi-key lookups**, and is optimized for high-performance client caching.

---

## Features

- **Primary Storage**: `TMap<TArray<uint8>, FRowEntry<RowType>>` for serialized keys.
- **Unique Constraints**: Enforce one-to-one mapping between a column and rows.
- **B-Tree Multi-Key Indices**: Allow one-to-many mapping between a key and rows.
- **Fast Lookups**: O(1) for unique index, O(log n) for B-Tree lookups.
- **Full Row Extraction**: Retrieve all cached rows in bulk.
- **Blueprint-ready**: The design is `TFunction`-based for easy integration with UE types.

---

## Internal Usage

### Add a Unique Constraint
```cpp
template<typename ColType>
void AddUniqueConstraint(
    const FString& Name,
    TFunction<ColType(const RowType&)> ExtractColumn);
```
- Must be called **before** populating `Entries`.
- Fails if the same constraint name is registered twice.

---

### Add a Multi-Key B-Tree Index
```cpp
template<typename KeyType>
void AddMultiKeyBTreeIndex(
    const FString& Name,
    TFunction<KeyType(const RowType&)> ExtractKey);
```
- Supports multiple rows per key.
- Uses a B-Tree for efficient range and point queries.

---

### Find by Unique Index
```cpp
template<typename KeyType>
const RowType* FindByUniqueIndex(
    const FString& Name,
    const KeyType& Key) const;
```
- Returns a single row pointer or `nullptr` if not found.

---

### Find by Multi-Key B-Tree Index
```cpp
template<typename KeyType>
void FindByMultiKeyBTreeIndex(
    TArray<RowType>& OutResults,
    const FString& Name,
    const KeyType& Key) const;
```
- Uses serialized key lookups internally.
---

### Get All Values
```cpp
void GetValues(TArray<RowType>& AllRows) const;
```
- Fills `AllRows` with copies of every row in the cache.

---

## Example Usage
```cpp
FTableCache<FMessage> Table;

// Add indices before inserting data
Table->AddUniqueConstraint<FSpacetimeDBIdentity>("sender", [](const FMessageType& Row) -> FSpacetimeDBIdentity {
        return Row.Sender;
        });

using FSenderTextKey = TTuple<FSpacetimeDBIdentity, FString>;
Table->AddMultiKeyBTreeIndex<FSenderTextKey>(
    TEXT("sender_text"),
    [](const FMessageType& Msg) { return MakeTuple(Msg.Sender, Msg.Text); }
);

// Query examples
const FMessage* Found = Table.FindByUniqueIndex(TEXT("MessageID"), TEXT("abc-123"));

using FSenderTextKey = TTuple<FSpacetimeDBIdentity, FString>;
TArray<FMessageType> Results;
Table->FindByMultiKeyBTreeIndex<FSenderTextKey>(
    Results,
    TEXT("sender_text"),
    MakeTuple(Sender, Message)
);

// Retrieve all rows
TArray<FMessage> AllRows;
Table.GetValues(AllRows);
```

---

## Notes

- `TArray<uint8>` keys allow serialized identifiers (network-friendly).
- Adding indices after inserting rows is **not supported** without manual rebuild.
- B-Tree indices can later be extended for **range queries**.ed to a **single column** per call.