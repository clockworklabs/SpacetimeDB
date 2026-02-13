#pragma once
#include "CoreMinimal.h"
#include "TableHandle.h"

/* ============================================================================ *
 *  UniqueConstraintHandle.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  Convenience helper to call Find() with a typed column value instead of the
 *  void* required by IUniqueIndex.
 * ============================================================================ */
template<typename RowType, typename ColType>
class FUniqueConstraintHandle
{
public:
    FTableHandle<RowType> Table;
    FString Constraint;

    FUniqueConstraintHandle(const FTableHandle<RowType>& T, const FString& C)
        : Table(T), Constraint(C) { }

    /** Return the row (if any) matching Key wrapped in TOptional. */
    TOptional<RowType> Find(const ColType& Key) const
    {
        const RowType* Found = Table.FindUnique(Constraint, &Key);
        return Found ? TOptional<RowType>(*Found) : TOptional<RowType>();
    }
};
