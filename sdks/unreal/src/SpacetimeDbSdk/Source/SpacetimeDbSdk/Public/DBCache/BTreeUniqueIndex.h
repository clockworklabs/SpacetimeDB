
#pragma once
#include <map>
#include "IUniqueIndex.h"

// A multi-key index implementation that maps a key to one or more serialized row identifiers.
// This is typically used for non-unique indexing (one key → many rows) in the client table cache.
template<typename RowType, typename KeyType>
class FMultiKeyBTreeIndex : public IMultiKeyIndex<RowType>
{
public:
    // Function that extracts the index key from a row instance.
    // For example, given a FMessage row, it might return Msg.Sender or a tuple of fields.
    TFunction<KeyType(const RowType&)> ExtractKey;

    // Maps a key to one or more serialized primary keys (TArray<uint8>) for rows that match.
    // The serialized keys can then be used to retrieve full rows from the table cache.
    TMultiMap<KeyType, TArray<uint8>> KeyToSerialized;

    // Temporary buffer to store found keys during a query.
    // Mutable so that FindKeys (which is const) can reuse this array without reallocating each time.
    mutable TArray<TArray<uint8>> MutableFoundKeys;

    // Constructor — stores the key extraction function.
    explicit FMultiKeyBTreeIndex(TFunction<KeyType(const RowType&)> InExtractKey)
        : ExtractKey(InExtractKey)
    {
    }

    /**
     * Adds a row to the multi-key index by mapping its extracted key to the serialized primary key.
     *
     * @param SerializedKey  Serialized representation of the row's primary key.
     * @param Row            Shared pointer to the row being added.
     */
    virtual void AddRow(const TArray<uint8>& SerializedKey, const TSharedPtr<RowType>& Row) override
    {
        // Extract the key from the row and add an entry mapping it to the serialized key.
        KeyToSerialized.Add(ExtractKey(*Row), SerializedKey);
    }


    /**
     * Removes a single mapping from the multi-key index matching the extracted key and serialized key.
     *
     * @param SerializedKey  Serialized representation of the row's primary key.
     * @param Row            Shared pointer to the row being removed.
     */
    virtual void RemoveRow(const TArray<uint8>& SerializedKey, const TSharedPtr<RowType>& Row) override
    {
        // Remove one occurrence of the pair (extracted key, serialized key) from the index.
        KeyToSerialized.RemoveSingle(ExtractKey(*Row), SerializedKey);
    }


    /**
     * Finds all serialized keys associated with the given index key.
     *
     * @param KeyPtr  Pointer to the key value (type-erased as void*).
     * @return        Pointer to an array of serialized keys matching the given key.
     */
    virtual const TArray<TArray<uint8>>* FindKeys(const void* KeyPtr) const override
    {
        // Cast the void pointer back to the actual key type.
        const KeyType* TypedKey = static_cast<const KeyType*>(KeyPtr);

        // Clear the temporary array to store matching keys.
        MutableFoundKeys.Reset();

        // Retrieve all serialized keys mapped to the given index key.
        KeyToSerialized.MultiFind(*TypedKey, MutableFoundKeys);

        // Return the pointer to the results array owned by this object.
        return &MutableFoundKeys;
    }

};