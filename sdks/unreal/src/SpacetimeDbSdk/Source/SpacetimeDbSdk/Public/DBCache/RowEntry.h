#pragma once
#include "CoreMinimal.h"

/* ============================================================================ *
 *  RowEntry.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  Mirrors the Rust `RowEntry<Row>` struct.
 *
 *  • Stores a single deserialized row value (RowType).
 *  • Tracks a reference‑count so that multiple overlapping query subscriptions
 *    can reference the same row without duplicating memory. When the ref‑count
 *    drops to zero the row can be safely removed from the table cache.
 * ============================================================================ */

//template<typename RowType>
//struct FRowEntry
//{
//    /** The row value copied from the network payload or database layer. */
//    RowType Row;
//    //TSharedPtr<RowType> Row; 
//
//    /** How many active subscriptions currently reference this row. */
//    uint32  RefCount{0};
//
//    FRowEntry() = default;
//    FRowEntry(const RowType& InRow, uint32 InRefCount = 0)
//        : Row(InRow), RefCount(InRefCount) {}
//};

template<typename RowType>
struct FRowEntry
{
    /** Shared row data (used by indices and cache) */
    TSharedPtr<RowType> Row;

    /** Reference count for this row */
    int32 RefCount = 0;

    FRowEntry(const TSharedPtr<RowType>& InRow, int32 InRefCount)
        : Row(InRow), RefCount(InRefCount)
    {
    }
};