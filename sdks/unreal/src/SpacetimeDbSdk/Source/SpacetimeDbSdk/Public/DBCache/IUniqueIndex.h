#pragma once
/* ============================================================================ *
 *  IUniqueIndex.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  Polymorphic interface replicating Rust's `UniqueIndexDyn<Row>` trait.
 *
 *  • A unique index guarantees at most one row per key.
 *  • Implementations must be able to Add, Remove and Find rows based on the
 *    value of a particular column.
 * ============================================================================ */

template<typename RowType>
class IBaseIndex
{
public:
    virtual ~IBaseIndex() = default;

    /**
     * Adds a row to the index.
     * Implementation should extract the key from the row and store it for lookups.
     *
     * @param Row  Shared pointer to the row being added.
     */
    virtual void AddRow(TSharedPtr<RowType> Row) = 0;

    /**
     * Removes a row from the index.
     * Implementation should remove any key mapping for this row.
     *
     * @param Row  Shared pointer to the row being removed.
     */
    virtual void RemoveRow(TSharedPtr<RowType> Row) = 0;

    /**
     * Finds a single row by the given key pointer.
     *
     * @param KeyPtr  Pointer to the key value (cast to correct type inside the function).
     * @return        Shared pointer to the matching row, or nullptr if not found.
     */
    virtual TSharedPtr<const RowType> FindRow(const void* KeyPtr) const = 0;

};

template<typename RowType>
class IMultiKeyIndex
{
public:
    virtual ~IMultiKeyIndex() = default;

    /**
     * Adds a row to the index using its serialized key and the row's data.
     * Implementations should map the extracted key to the serialized representation.
     *
     * @param SerializedKey  Binary key representing the row in serialized form.
     * @param Row            Shared pointer to the row being added.
     */
    virtual void AddRow(const TArray<uint8>& SerializedKey, const TSharedPtr<RowType>& Row) = 0;

    /**
     * Removes a row from the index using its serialized key and the row's data.
     * Implementations should ensure the mapping is cleaned up.
     *
     * @param SerializedKey  Binary key representing the row in serialized form.
     * @param Row            Shared pointer to the row being removed.
     */
    virtual void RemoveRow(const TArray<uint8>& SerializedKey, const TSharedPtr<RowType>& Row) = 0;

    /**
     * Finds all serialized keys that match the given key pointer.
     *
     * @param KeyPtr  Pointer to the key value (cast to the correct key type inside the function).
     * @return        Pointer to an array of serialized keys that match, or nullptr if none found.
     */
    virtual const TArray<TArray<uint8>>* FindKeys(const void* KeyPtr) const = 0;

};

