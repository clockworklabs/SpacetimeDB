#pragma once
#include "CoreMinimal.h"
#include "IUniqueIndex.h"

/* ============================================================================ *
 *  UniqueIndex.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  Concrete implementation of IBaseIndex.
 *
 *  Template Parameters
 *  -------------------
 *  RowType : Struct representing a table row.
 *  ColType : Type of the unique column (e.g. FString, int32).
 *
 *  Internally stores a TMap<ColType, RowType>.  Duplicate keys cause a fatal
 *  error (mimicking Rust's panic! on violation of a unique constraint).
 * ============================================================================ */

#include "Containers/Map.h"
#include "Templates/SharedPointer.h"

template<typename RowType, typename ColType>
class FUniqueIndex : public IBaseIndex<RowType>
{
public:
    // Maps unique column values to their corresponding row shared pointers.
    // Enforces one-to-one relationship between column value and row.
    TMap<ColType, TSharedPtr<RowType>> Rows;

    // Function to extract the unique key from a row.
    // Used when adding, removing, or searching rows.
    TFunction<ColType(const RowType&)> GetKeyFunc;

    // Constructor accepting a key extraction function.
    // Uses MoveTemp for efficient move semantics.
    explicit FUniqueIndex(TFunction<ColType(const RowType&)> InGetKeyFunc)
        : GetKeyFunc(MoveTemp(InGetKeyFunc))
    {
    }

    // Default virtual destructor to allow proper cleanup in derived classes.
    virtual ~FUniqueIndex() = default;


    /**
     * Adds a row to the unique index.
     * If the key already exists, the existing entry is replaced.
     *
     * @param Row  Shared pointer to the row being added.
     */
    virtual void AddRow(TSharedPtr<RowType> Row) override
    {
        // Extract the unique key from the row.
        const ColType Key = GetKey(*Row);

        // Insert or update the map with the new row pointer.
        Rows.FindOrAdd(Key) = Row; // Replaces existing entry if key exists.
    }


    /**
     * Removes a row from the unique index based on its unique key.
     *
     * @param Row  Shared pointer to the row being removed.
     */
    virtual void RemoveRow(TSharedPtr<RowType> Row) override
    {
        // Extract the unique key from the row.
        const ColType Key = GetKey(*Row);

        // Remove the entry with this key from the map.
        int32 Removed = Rows.Remove(Key);
    }


    /**
     * Finds a row in the unique index by the given key.
     *
     * @param KeyPtr  Pointer to the key (type-erased as void*).
     * @return        Shared pointer to the const matching row, or nullptr if not found.
     */
    virtual TSharedPtr<const RowType> FindRow(const void* KeyPtr) const override
    {
        if (!KeyPtr) // Validate input pointer
        {
            return nullptr;
        }

        // Cast the void pointer back to the actual key type
        const ColType& Key = *static_cast<const ColType*>(KeyPtr);

        // Search for the key in the map
        const TSharedPtr<RowType>* Found = Rows.Find(Key);

        // If found and valid, log and return the const shared pointer
        if (Found && Found->IsValid())
        {
            return StaticCastSharedPtr<const RowType>(*Found);
        }

        // Return nullptr if not found or invalid
        return nullptr;
    }


    /**
     * Extracts the unique key from the given row using the stored key extraction function.
     *
     * @param Row  The row to extract the key from.
     * @return     The extracted key of type ColType.
     */
    virtual ColType GetKey(const RowType& Row) const
    {
        return GetKeyFunc(Row);
    }

};

/**
 * Computes a hash for a byte array using CRC32.
 *
 * @param Bytes  The byte array to hash.
 * @return       32-bit hash value.
 */
FORCEINLINE uint32 GetTypeHash(const TArray<uint8>& Bytes)
{
    return FCrc::MemCrc32(Bytes.GetData(), Bytes.Num());
}
