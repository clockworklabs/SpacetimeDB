#pragma once
#include "CoreMinimal.h"

/* ============================================================================ *
 *  TableAppliedDiff.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  Captures the semantic result of applying a low-level diff (inserts/deletes)
 *  to a table cache. Rows that transition from dead to live are inserts, live
 *  to dead are deletes, and a delete+insert with the same primary key is
 *  surfaced as an update pair.
 *
 *  This is the SDK's lower-copy diff representation. Direct C++ consumers read
 *  row payloads from TSharedPtr-backed arrays; generated dynamic delegates still
 *  broadcast value references for Blueprint and existing dynamic delegate code.
 * ============================================================================ */
template<typename RowType>
struct FTableAppliedDiff
{
    TArray<TSharedPtr<RowType>> Deletes;
    TArray<TSharedPtr<RowType>> Inserts;

    TArray<TSharedPtr<RowType>> UpdateDeletes;
    TArray<TSharedPtr<RowType>> UpdateInserts;

    bool bPrimaryKeyUpdatesClassified = false;

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
        if (bPrimaryKeyUpdatesClassified)
        {
            checkf(UpdateDeletes.Num() == UpdateInserts.Num(), TEXT("Pre-classified primary-key update diff arrays are mismatched."));
            return;
        }
        if (Deletes.IsEmpty() || Inserts.IsEmpty()) return;

        const int32 DeleteCount = Deletes.Num();
        const int32 InsertCount = Inserts.Num();
        TMap<KeyType, TArray<int32, TInlineAllocator<1>>> DeletePK;
        DeletePK.Reserve(Deletes.Num());
        for (int32 DeleteIndex = DeleteCount - 1; DeleteIndex >= 0; --DeleteIndex)
        {
            const TSharedPtr<RowType>& DeletedRow = Deletes[DeleteIndex];
            checkf(DeletedRow.IsValid(), TEXT("Invalid deleted row while deriving SpacetimeDB table updates."));
            const KeyType PK = DerivePK(*DeletedRow);
            DeletePK.FindOrAdd(PK).Add(DeleteIndex);
        }

        const int32 MaxUpdatePairs = FMath::Min(DeleteCount, InsertCount);
        TArray<uint8> MatchedDeletes;
        TArray<uint8> MatchedInserts;
        MatchedDeletes.Init(0, DeleteCount);
        MatchedInserts.Init(0, InsertCount);
        UpdateDeletes.Reserve(UpdateDeletes.Num() + MaxUpdatePairs);
        UpdateInserts.Reserve(UpdateInserts.Num() + MaxUpdatePairs);
        int32 MatchedPairCount = 0;
        for (int32 InsertIndex = 0; InsertIndex < InsertCount; ++InsertIndex)
        {
            const TSharedPtr<RowType>& InsertedRow = Inserts[InsertIndex];
            checkf(InsertedRow.IsValid(), TEXT("Invalid inserted row while deriving SpacetimeDB table updates."));
            KeyType PK = DerivePK(*InsertedRow);
            if (TArray<int32, TInlineAllocator<1>>* DeleteIndices = DeletePK.Find(PK))
            {
                checkf(!DeleteIndices->IsEmpty(), TEXT("Empty deleted row index list while deriving SpacetimeDB table updates."));
                const int32 DeleteIndex = DeleteIndices->Pop(EAllowShrinking::No);
                checkf(Deletes.IsValidIndex(DeleteIndex), TEXT("Invalid deleted row index while deriving SpacetimeDB table updates."));
                UpdateDeletes.Add(Deletes[DeleteIndex]);
                UpdateInserts.Add(InsertedRow);
                MatchedDeletes[DeleteIndex] = 1;
                MatchedInserts[InsertIndex] = 1;
                if (DeleteIndices->IsEmpty())
                {
                    DeletePK.Remove(PK);
                }
                ++MatchedPairCount;
            }
        }

        if (MatchedPairCount == 0)
        {
            return;
        }

        TArray<TSharedPtr<RowType>> RemainingDeletes;
        TArray<TSharedPtr<RowType>> RemainingInserts;
        RemainingDeletes.Reserve(DeleteCount - MatchedPairCount);
        RemainingInserts.Reserve(InsertCount - MatchedPairCount);
        for (int32 DeleteIndex = 0; DeleteIndex < DeleteCount; ++DeleteIndex)
        {
            if (MatchedDeletes[DeleteIndex] == 0)
            {
                RemainingDeletes.Add(MoveTemp(Deletes[DeleteIndex]));
            }
        }
        for (int32 InsertIndex = 0; InsertIndex < InsertCount; ++InsertIndex)
        {
            if (MatchedInserts[InsertIndex] == 0)
            {
                RemainingInserts.Add(MoveTemp(Inserts[InsertIndex]));
            }
        }
        Deletes = MoveTemp(RemainingDeletes);
        Inserts = MoveTemp(RemainingInserts);
    }
};
