#pragma once
#include "CoreMinimal.h"
#include "TableCache.h"
#include "TableAppliedDiff.h"

/* ============================================================================ *
 *  ClientCache.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  Owns a collection of FTableCache instances—one per subscribed table name.
 *  Provides helper to apply (insert/delete) diffs arriving from the network.
 * ============================================================================ */
template<typename RowType>
class UClientCache
{
public:
    /**
     * Shared pointer to the cached table data for this row type.
     * Currently supports storing only one table instance per RowType.
     * For multiple tables by name, consider using a map keyed by table name.
     */
    TSharedPtr<FTableCache<RowType>> Table;


    /**
     * Retrieves the existing table cache or creates a new one if none exists.
     *
     * @param Name  The name of the table (used here for validation).
     * @return      Shared pointer to the table cache instance or nullptr if name is empty.
     */
    TSharedPtr<FTableCache<RowType>> GetOrAdd(const FString& Name)
    {
        if (Name.IsEmpty())
        {
            UE_LOG(LogTemp, Warning, TEXT("GetOrAdd called with empty table name."));
            return nullptr;
        }

        // If the table already exists, return it
        if (Table.IsValid())
        {
            return Table;
        }

        // Otherwise, create a new table cache instance and store it
        auto NewCache = MakeShared<FTableCache<RowType>>();
        Table = NewCache;
        return NewCache;
    }

    /**
     * Retrieves a const shared pointer to the table cache if it exists.
     *
     * @param Name  The name of the table (used here for validation).
     * @return      Const shared pointer to the table cache or nullptr if not found/empty name.
     */
    TSharedPtr<const FTableCache<RowType>> GetTable(const FString& Name) const
    {
        if (Name.IsEmpty())
        {
            UE_LOG(LogTemp, Warning, TEXT("GetTable called with empty table name."));
            return nullptr;
        }

        // Return the existing table if valid, otherwise nullptr
        if (Table.IsValid())
        {
            return Table;
        }
        return nullptr;
    }


    /**
     *  Apply Inserts + Deletes to the specified table.
     *  Inserts: increment refCount, add new entry when needed.
     *  Deletes: decrement refCount, remove when it reaches 0.
     */
    FTableAppliedDiff<RowType> ApplyDiff(
        const FString& Name,
        const TArray<TPair<TArray<uint8>, RowType>>& Inserts,
        const TArray<TArray<uint8>>& Deletes)
    {
        if (Name.IsEmpty())
        {
            UE_LOG(LogTemp, Error, TEXT("ApplyDiff called with empty table name."));
            return FTableAppliedDiff<RowType>();
        }

        if (!Table.IsValid())
        {
            UE_LOG(LogTemp, Error, TEXT("Failed to create or retrieve table: %s"), *Name);
            return FTableAppliedDiff<RowType>();
        }

        FTableAppliedDiff<RowType> Diff;

        // Map of deleted SerializedBytes -> (Key, Row)
        // The key type is now generic TArray<uint8>
        TMap<TArray<uint8>, TPair<TArray<uint8>, TSharedPtr<RowType>>> DeletedEntries;

        // Phase 1: Pre-process Deletes
        for (const TArray<uint8>& Key : Deletes)
        {

            FRowEntry<RowType>* Entry = Table->Entries.Find(Key);
            if (!Entry) continue;

            // Decrement refcount and store the entry if it's about to be deleted
            if (--Entry->RefCount == 0)
            {
                DeletedEntries.Add(Key, TPair<TArray<uint8>, TSharedPtr<RowType>>(Key, Entry->Row));
            }
        }

        // Phase 2: Process Inserts and Updates
        for (const auto& Ins : Inserts)
        {
            const TArray<uint8>& Key = Ins.Key;
            const RowType& Row = Ins.Value;

            TSharedPtr<RowType> NewRow = MakeShared<RowType>(Row);

            // This is a new row or an update to an existing row that was not deleted.
            FRowEntry<RowType>* Entry = Table->Entries.Find(Key);
            if (!Entry)
            {
                // True insert
                Table->Entries.Add(Key, FRowEntry<RowType>{NewRow, 1});
            }
            else
            {
                // True update
                Table->Entries.Add(Key, FRowEntry<RowType>{NewRow, Entry->RefCount + 1});
            }

            Diff.Inserts.Add(Key, *NewRow);
        }

        // Phase 3: Finalize Deletes and Update Indices
        for (const auto& KeyValue : DeletedEntries)
        {
            // Add to diff before removal
            Diff.Deletes.Add(KeyValue.Key, *KeyValue.Value.Value);
            Table->Entries.Remove(KeyValue.Key);
        }

        // Now, update all indices with the completed diff
        for (const auto& DeletePair : Diff.Deletes)
        {
            for (auto& IndexPair : Table->UniqueIndices)
            {
                // Assuming RemoveRow takes the TSharedPtr<RowType> directly
                IndexPair.Value->RemoveRow(MakeShared<RowType>(DeletePair.Value));
            }
        }

        for (const auto& InsertPair : Diff.Inserts)
        {
            for (auto& IndexPair : Table->UniqueIndices)
            {
                // Assuming AddRow takes TSharedPtr<RowType> directly
                IndexPair.Value->AddRow(MakeShared<RowType>(InsertPair.Value));
            }
        }

        // And for BtreeIndices...
        for (auto& Pair : Table->BTreeIndices)
        {
            for (const auto& DeletePair : Diff.Deletes)
            {
                Pair.Value->RemoveRow(DeletePair.Key, MakeShared<RowType>(DeletePair.Value));
            }

            for (const auto& InsertPair : Diff.Inserts)
            {
                Pair.Value->AddRow(InsertPair.Key, MakeShared<RowType>(InsertPair.Value));
            }
        }

        return Diff;
    }
};
