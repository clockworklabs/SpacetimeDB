#pragma once
#include "CoreMinimal.h"

/* ============================================================================ *
 *  TableAppliedDiff.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  Captures the semantic result of applying a low‑level diff (inserts/deletes)
 *  to a table cache.  Rows that transition from dead→live are inserts, live→dead
 *  are deletes, and a delete+insert with the same primary‑key is surfaced as an
 *  update pair.
 * ============================================================================ */
template<typename RowType>
struct FTableAppliedDiff
{
    // SerializedKey -> Row copy. Keeping the rows by value ensures
    // the memory stays valid even if the underlying table reallocates
    // or removes entries while this diff is alive.
    TMap<TArray<uint8>, RowType> Deletes;
    TMap<TArray<uint8>, RowType> Inserts;

    // Parallel arrays for (old, new) row update pairs.
    TArray<RowType> UpdateDeletes;
    TArray<RowType> UpdateInserts;

    bool IsEmpty() const
    {
        return Deletes.IsEmpty() && Inserts.IsEmpty() &&
            UpdateDeletes.IsEmpty() && UpdateInserts.IsEmpty();
    }

    /**
     *  Examine Inserts and Deletes, detect primary‑key matches and move them
     *  into Update* arrays. The key extractor returns a value type used for
     *  comparison.
     */
    template<typename KeyType>
    void DeriveUpdatesByPrimaryKey(TFunctionRef<KeyType(const RowType&)> DerivePK)
    {
        if (Deletes.IsEmpty()) return;

        // Build PK->(key,row) map for deletes.
        TMap<KeyType, TPair<TArray<uint8>, RowType>> DeletePK;
        for (const auto& Pair : Deletes)
        {
            DeletePK.Add(DerivePK(Pair.Value), { Pair.Key, Pair.Value });
        }

        // Scan inserts for matching PKs.
        TArray<TArray<uint8>> DeleteKeys;
        TArray<TArray<uint8>> InsertKeys;
        for (const auto& Pair : Inserts)
        {
            KeyType PK = DerivePK(Pair.Value);
            if (const auto* Found = DeletePK.Find(PK))
            {
                UpdateDeletes.Add(Found->Value);
                UpdateInserts.Add(Pair.Value);
                DeleteKeys.Add(Found->Key);
                InsertKeys.Add(Pair.Key);
            }
        }

        // Remove update pairs from base maps.
        for (const auto& K : DeleteKeys)
        {
            Deletes.Remove(K);
        }
        for (const auto& K : InsertKeys)
        {
            Inserts.Remove(K);
        }
    }
};
