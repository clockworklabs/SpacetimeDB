#pragma once
#include "CoreMinimal.h"
#include "RowEntry.h"
#include "UniqueIndex.h"
#include "BTreeUniqueIndex.h"

/* ============================================================================ *
 *  TableCache.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  An in‑memory mirror of a single database table.
 *
 *  Keyed by serialized byte blobs (BSATN) so we can hash/compare cheaply even
 *  for row structs containing floats or other non‑hashable fields.
 * ============================================================================ */
template<typename RowType>
class FTableCache
{
public:

    /**
     * Main storage of table rows keyed by their serialized primary key.
     * TArray<uint8> is used to allow arbitrary binary keys.
     */
    TMap<TArray<uint8>, FRowEntry<RowType>> Entries;

    /**
     * Map of unique index name -> unique index object.
     * Each unique index enforces one-column uniqueness.
     */
    TMap<FString, TSharedPtr<IBaseIndex<RowType>>> UniqueIndices;

    /**
     * Map of multi-key B-Tree index name -> index object.
     * Used for efficient lookups on non-unique columns or composite keys.
     */
    TMap<FString, TSharedPtr<IMultiKeyIndex<RowType>>> BTreeIndices;

    /* --------------------------------------------------------------------- */

    /**
     * Adds a unique constraint (unique index) to the table.
     *
     * This enforces that all values in the specified column are unique.
     * Must be called before any entries are populated.
     *
     * @tparam ColType      Type of the column used for uniqueness checks.
     * @param Name          Unique name for the constraint.
     * @param ExtractColumn Function that extracts the column value from a given row.
     */
    template<typename ColType>
    void AddUniqueConstraint(
        const FString& Name,
        TFunction<ColType(const RowType&)> ExtractColumn)
    {
        // Ensure this is called before data population
        check(Entries.IsEmpty());

        // Prevent duplicate unique constraints
        if (UniqueIndices.Contains(Name))
        {
            UE_LOG(LogTemp, Error, TEXT("Duplicate unique constraint: %s"), *Name);
            return;
        }

        // Create and store the unique index
        UniqueIndices.Add(Name,
            MakeShared<FUniqueIndex<RowType, ColType>>(ExtractColumn));
    }


    /**
     * Adds a new multi-key B-Tree index to the table.
     *
     * @tparam KeyType      Type of the key to extract and store in the index.
     * @param Name          Unique name for the index.
     * @param ExtractKey    Function that extracts the key from a given row.
     */
    template<typename KeyType>
    void AddMultiKeyBTreeIndex(
        const FString& Name,
        TFunction<KeyType(const RowType&)> ExtractKey)
    {
        // Prevent duplicate index names
        if (BTreeIndices.Contains(Name))
        {
            UE_LOG(LogTemp, Error, TEXT("Duplicate B-Tree index: %s"), *Name);
            return;
        }

        // Create the B-Tree index and register it
        TSharedPtr<IMultiKeyIndex<RowType>> NewIndex = MakeShared<FMultiKeyBTreeIndex<RowType, KeyType>>(ExtractKey);
        BTreeIndices.Add(Name, NewIndex);
    }


    /**
     * Finds a row by its unique index key.
     *
     * @tparam KeyType   The type of the key to search for.
     * @param Name       The unique index name.
     * @param Key        The key value to find.
     * @return           Pointer to the matching row or nullptr if not found.
     */
    template<typename KeyType>
    const RowType* FindByUniqueIndex(
        const FString& Name,
        const KeyType& Key) const
    {
        // Look up the unique index by name
        const auto* Ptr = UniqueIndices.Find(Name);
        if (!Ptr || !Ptr->IsValid()) // If index missing or invalid, return nullptr
        {
            return nullptr;
        }

        // The interface can't call templated FindRowEx, but it can call FindRow with a void* key.
        // FindRow will internally static_cast the key pointer to the correct KeyType.
        TSharedPtr<const RowType> FoundRowPtr = (*Ptr)->FindRow(&Key);

        // Return raw pointer; ownership remains internal
        return FoundRowPtr.Get();
    }

    /**
    * Finds all rows from a multi-key B-Tree index that match the given key.
    * Uses output parameter pattern to avoid unnecessary copies.
    * Works with any RowType - use pointer types (UClassName*) for UObject-derived classes.
    *
    * @tparam KeyType   The type of the key to search for in the index.
    * @param OutResults Output array that will be filled with matching rows.
    * @param Name       The name of the B-Tree index to query.
    * @param Key        The key value to search for.
    */
    template<typename KeyType>
    void FindByMultiKeyBTreeIndex(TArray<RowType>& OutResults, const FString& Name, const KeyType& Key) const
    {
        // Clear output array first
        OutResults.Reset();

        // Find the index object by its name.
        const auto* IndexPtr = BTreeIndices.Find(Name);
        if (!IndexPtr || !(*IndexPtr)) // Return empty if index is missing or invalid.
        {
            return;
        }

        // Retrieve all serialized keys that match the provided search key.
        const TArray<TArray<uint8>>* SerializedKeys = (*IndexPtr)->FindKeys(&Key);
        if (!SerializedKeys) // Return empty if no matching keys found.
        {
            return;
        }

        // Loop through each serialized key to find the corresponding row entry.
        for (const TArray<uint8>& SerializedKey : *SerializedKeys)
        {
            const FRowEntry<RowType>* Entry = Entries.Find(SerializedKey);
            if (Entry && Entry->Row.IsValid()) // If the entry exists, add copy to the results.
            {
                OutResults.Add(*Entry->Row);
            }
        }
    }
    
    /**
     * Retrieves all rows currently stored in the table cache.
     *
     * @param AllRows   Output array to be filled with copies of all rows.
     */
    void GetValues(TArray<RowType>& AllRows) const
    {
        // Temporary array to hold all FRowEntry objects from the Entries map
        TArray<FRowEntry<RowType>> TempRowEntries;
        Entries.GenerateValueArray(TempRowEntries);

        // Clear the output array to ensure no old data remains
        AllRows.Empty();

        // Extract the actual Row objects from each FRowEntry and add them to AllRows
        for (const FRowEntry<RowType>& Entry : TempRowEntries)
        {
            AllRows.Add(Entry.Row);  // Add by value; assumes FRowEntry.Row is RowType or convertible
        }
    }

};
