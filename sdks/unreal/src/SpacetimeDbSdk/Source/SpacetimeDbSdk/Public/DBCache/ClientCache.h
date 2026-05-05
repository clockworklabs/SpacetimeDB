#pragma once
#include "CoreMinimal.h"
#include "BSATN/UESpacetimeDB.h"
#include "TableCache.h"
#include "TableAppliedDiff.h"
#include "WithBsatn.h"

#include <type_traits>
#include <utility>

namespace UE::SpacetimeDB
{
enum class ETableCacheApplyMode : uint8
{
    PersistentIndexed,
    DirectNativeDiff
};

enum class ERuntimeBTreeIndexApplyMode : uint8
{
    Apply,
    Skip
};

template<typename RowType>
struct TCompactPrimaryKeyTraits
{
    static constexpr bool bEnabled = false;
    using KeyType = uint64;

    static KeyType GetKey(const RowType& Row)
    {
        (void)Row;
        static_assert(bEnabled, "SpacetimeDB compact cache key trait is not generated for this row type.");
        return 0;
    }

    static const TCHAR* GetUniqueIndexName()
    {
        static_assert(bEnabled, "SpacetimeDB compact cache key trait is not generated for this row type.");
        return TEXT("");
    }
};

template<typename RowType>
struct TTableCachePolicy
{
    static constexpr ETableCacheApplyMode ApplyMode = ETableCacheApplyMode::PersistentIndexed;

    static ERuntimeBTreeIndexApplyMode GetRuntimeBTreeIndexApplyMode(const FString& IndexName)
    {
        (void)IndexName;
        return ERuntimeBTreeIndexApplyMode::Apply;
    }
};
}

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

    void SetApplyMode(UE::SpacetimeDB::ETableCacheApplyMode InApplyMode)
    {
        ApplyMode = InApplyMode;
    }

    UE::SpacetimeDB::ETableCacheApplyMode GetApplyMode() const
    {
        return ApplyMode;
    }

    void SetRuntimeBTreeIndexApplyMode(
        const FString& IndexName,
        UE::SpacetimeDB::ERuntimeBTreeIndexApplyMode InApplyMode)
    {
        checkf(!IndexName.IsEmpty(), TEXT("Cannot configure runtime B-Tree index policy for an empty index name."));
        switch (InApplyMode)
        {
        case UE::SpacetimeDB::ERuntimeBTreeIndexApplyMode::Apply:
            RuntimeBTreeIndexApplyModes.Add(IndexName, InApplyMode);
            break;
        case UE::SpacetimeDB::ERuntimeBTreeIndexApplyMode::Skip:
            RuntimeBTreeIndexApplyModes.Add(IndexName, InApplyMode);
            break;
        default:
            checkf(false, TEXT("Unknown runtime B-Tree index apply mode for index '%s'."), *IndexName);
            break;
        }
    }

    void ClearRuntimeBTreeIndexApplyModes()
    {
        RuntimeBTreeIndexApplyModes.Reset();
    }


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


    FTableAppliedDiff<RowType> ApplyDiffByPrimaryKey(
        const FString& Name,
        TArray<FWithBsatn<RowType>>&& Inserts,
        TArray<FWithBsatn<RowType>>&& Deletes,
        const TCHAR* ExpectedUniqueIndexName)
    {
        using FCompactPrimaryKeyTraits = UE::SpacetimeDB::TCompactPrimaryKeyTraits<RowType>;
        static_assert(FCompactPrimaryKeyTraits::bEnabled, "ApplyDiffByPrimaryKey requires a generated compact primary-key trait.");
        using KeyType = typename FCompactPrimaryKeyTraits::KeyType;

        checkf(!Name.IsEmpty(), TEXT("ApplyDiffByPrimaryKey called with empty table name."));
        checkf(Table.IsValid(), TEXT("ApplyDiffByPrimaryKey could not find table cache for %s."), *Name);
        checkf(ExpectedUniqueIndexName != nullptr && ExpectedUniqueIndexName[0] != TEXT('\0'),
            TEXT("ApplyDiffByPrimaryKey for %s requires a generated unique index name."), *Name);
        const FString ExpectedIndexName(ExpectedUniqueIndexName);
        checkf(Table->UniqueIndices.Contains(ExpectedIndexName),
            TEXT("ApplyDiffByPrimaryKey for %s requires generated unique index %s."),
            *Name,
            ExpectedUniqueIndexName);

        if (ShouldApplyDirectNativeDiff())
        {
            return BuildDirectDiffByPrimaryKey(Name, MoveTemp(Inserts), MoveTemp(Deletes));
        }

        struct FDeletedRow
        {
            TArray<uint8> CacheKey;
            TSharedPtr<RowType> Row;
            int32 PendingCount = 0;
            bool bUpdateApplied = false;
        };

        auto BuildCacheKey = [](const KeyType& Key)
        {
            static constexpr int32 CompactPrimaryKeyBytes = sizeof(KeyType);
            TArray<uint8> CacheKey;
            CacheKey.SetNumUninitialized(CompactPrimaryKeyBytes);
            uint64 Remaining = Key;
            for (int32 ByteIndex = 0; ByteIndex < CompactPrimaryKeyBytes; ++ByteIndex)
            {
                CacheKey[ByteIndex] = static_cast<uint8>(Remaining & 0xffu);
                Remaining >>= 8;
            }
            return CacheKey;
        };

        auto RemoveFromIndices = [this](const TArray<uint8>& Key, const TSharedPtr<RowType>& Row)
        {
            checkf(Row.IsValid(), TEXT("Cannot remove invalid row from table indices."));
            for (auto& IndexPair : Table->UniqueIndices)
            {
                IndexPair.Value->RemoveRow(Row);
            }
            for (auto& IndexPair : Table->BTreeIndices)
            {
                if (!ShouldApplyRuntimeBTreeIndex(IndexPair.Key))
                {
                    continue;
                }
                IndexPair.Value->RemoveRow(Key, Row);
            }
        };

        auto AddToIndices = [this](const TArray<uint8>& Key, const TSharedPtr<RowType>& Row)
        {
            checkf(Row.IsValid(), TEXT("Cannot add invalid row to table indices."));
            for (auto& IndexPair : Table->UniqueIndices)
            {
                IndexPair.Value->AddRow(Row);
            }
            for (auto& IndexPair : Table->BTreeIndices)
            {
                if (!ShouldApplyRuntimeBTreeIndex(IndexPair.Key))
                {
                    continue;
                }
                IndexPair.Value->AddRow(Key, Row);
            }
        };

        FTableAppliedDiff<RowType> Diff;
        Diff.bPrimaryKeyUpdatesClassified = true;
        Diff.Inserts.Reserve(Inserts.Num());
        Diff.Deletes.Reserve(Deletes.Num());
        Diff.UpdateDeletes.Reserve(FMath::Min(Inserts.Num(), Deletes.Num()));
        Diff.UpdateInserts.Reserve(FMath::Min(Inserts.Num(), Deletes.Num()));

        TMap<KeyType, FDeletedRow> DeletedRows;
        DeletedRows.Reserve(Deletes.Num());

        for (const FWithBsatn<RowType>& Delete : Deletes)
        {
            const KeyType PrimaryKey = FCompactPrimaryKeyTraits::GetKey(Delete.Row);
            const TArray<uint8> CacheKey = BuildCacheKey(PrimaryKey);
            FRowEntry<RowType>* Entry = Table->Entries.Find(CacheKey);
            if (!Entry)
            {
                continue;
            }

            checkf(Entry->RefCount > 0, TEXT("Table cache row for %s has invalid refcount before primary-key delete."), *Name);
            checkf(Entry->Row.IsValid(), TEXT("Table cache row for %s is invalid before primary-key delete."), *Name);
            FDeletedRow& Deleted = DeletedRows.FindOrAdd(PrimaryKey);
            if (!Deleted.Row.IsValid())
            {
                Deleted.CacheKey = CacheKey;
                Deleted.Row = Entry->Row;
            }
            checkf(Deleted.CacheKey == CacheKey, TEXT("Mismatched compact cache key for primary-key delete on %s."), *Name);
            ++Deleted.PendingCount;
            --Entry->RefCount;
            checkf(Entry->RefCount >= 0, TEXT("Table cache row for %s has negative refcount after primary-key delete."), *Name);
        }

        for (FWithBsatn<RowType>& Insert : Inserts)
        {
            const KeyType PrimaryKey = FCompactPrimaryKeyTraits::GetKey(Insert.Row);
            const TArray<uint8> CacheKey = BuildCacheKey(PrimaryKey);
            FDeletedRow* MatchingDelete = DeletedRows.Find(PrimaryKey);
            if (MatchingDelete && MatchingDelete->PendingCount > 0)
            {
                checkf(MatchingDelete->CacheKey == CacheKey,
                    TEXT("Mismatched compact cache key for primary-key update on %s."), *Name);
                FRowEntry<RowType>* Entry = Table->Entries.Find(CacheKey);
                checkf(Entry != nullptr, TEXT("Missing table cache row for primary-key update on %s."), *Name);
                checkf(Entry->RefCount >= 0, TEXT("Table cache row for %s has invalid refcount before primary-key update insert."), *Name);
                checkf(MatchingDelete->Row.IsValid(), TEXT("Invalid deleted row for primary-key update on %s."), *Name);

                ++Entry->RefCount;
                if (!MatchingDelete->bUpdateApplied)
                {
                    TSharedPtr<RowType> OldRow = MatchingDelete->Row;
                    TSharedPtr<RowType> NewRow = MakeShared<RowType>(MoveTemp(Insert.Row));
                    RemoveFromIndices(CacheKey, OldRow);
                    Entry->Row = NewRow;
                    AddToIndices(CacheKey, NewRow);

                    Diff.UpdateDeletes.Add(OldRow);
                    Diff.UpdateInserts.Add(NewRow);
                    MatchingDelete->bUpdateApplied = true;
                    MatchingDelete->Row = NewRow;
                }
                --MatchingDelete->PendingCount;
                continue;
            }

            FRowEntry<RowType>* ExistingEntry = Table->Entries.Find(CacheKey);
            if (ExistingEntry)
            {
                checkf(ExistingEntry->RefCount > 0,
                    TEXT("Primary-key insert for %s found an existing row with invalid refcount."), *Name);
                ++ExistingEntry->RefCount;
                continue;
            }

            TSharedPtr<RowType> NewRow = MakeShared<RowType>(MoveTemp(Insert.Row));
            Table->Entries.Add(CacheKey, FRowEntry<RowType>{NewRow, 1});
            AddToIndices(CacheKey, NewRow);
            Diff.Inserts.Add(NewRow);
        }

        for (const TPair<KeyType, FDeletedRow>& DeletedPair : DeletedRows)
        {
            const FDeletedRow& Deleted = DeletedPair.Value;
            if (Deleted.PendingCount <= 0)
            {
                continue;
            }

            FRowEntry<RowType>* Entry = Table->Entries.Find(Deleted.CacheKey);
            if (!Entry || Entry->RefCount > 0)
            {
                continue;
            }

            checkf(Deleted.Row.IsValid(), TEXT("Invalid deleted row for primary-key delete on %s."), *Name);
            checkf(Entry->RefCount == 0, TEXT("Primary-key delete for %s reached impossible refcount state."), *Name);
            RemoveFromIndices(Deleted.CacheKey, Deleted.Row);
            Diff.Deletes.Add(Deleted.Row);
            Table->Entries.Remove(Deleted.CacheKey);
        }

        return Diff;
    }

private:
    FTableAppliedDiff<RowType> BuildDirectDiffByPrimaryKey(
        const FString& Name,
        TArray<FWithBsatn<RowType>>&& Inserts,
        TArray<FWithBsatn<RowType>>&& Deletes)
    {
        using FCompactPrimaryKeyTraits = UE::SpacetimeDB::TCompactPrimaryKeyTraits<RowType>;
        static_assert(FCompactPrimaryKeyTraits::bEnabled, "BuildDirectDiffByPrimaryKey requires a generated compact primary-key trait.");
        using KeyType = typename FCompactPrimaryKeyTraits::KeyType;

        checkf(!Name.IsEmpty(), TEXT("BuildDirectDiffByPrimaryKey called with empty table name."));

        FTableAppliedDiff<RowType> Diff;
        Diff.bPrimaryKeyUpdatesClassified = true;
        Diff.Inserts.Reserve(Inserts.Num());
        Diff.Deletes.Reserve(Deletes.Num());
        Diff.UpdateDeletes.Reserve(FMath::Min(Inserts.Num(), Deletes.Num()));
        Diff.UpdateInserts.Reserve(FMath::Min(Inserts.Num(), Deletes.Num()));

        TMap<KeyType, TArray<TSharedPtr<RowType>>> DeletesByKey;
        DeletesByKey.Reserve(Deletes.Num());
        for (FWithBsatn<RowType>& Delete : Deletes)
        {
            const KeyType PrimaryKey = FCompactPrimaryKeyTraits::GetKey(Delete.Row);
            TArray<TSharedPtr<RowType>>& Rows = DeletesByKey.FindOrAdd(PrimaryKey);
            Rows.Add(MakeShared<RowType>(MoveTemp(Delete.Row)));
        }

        for (FWithBsatn<RowType>& Insert : Inserts)
        {
            const KeyType PrimaryKey = FCompactPrimaryKeyTraits::GetKey(Insert.Row);
            if (TArray<TSharedPtr<RowType>>* MatchingDeletes = DeletesByKey.Find(PrimaryKey))
            {
                checkf(!MatchingDeletes->IsEmpty(),
                    TEXT("Direct compact diff for %s found an empty delete bucket."),
                    *Name);
                TSharedPtr<RowType> OldRow = MatchingDeletes->Pop(EAllowShrinking::No);
                checkf(OldRow.IsValid(),
                    TEXT("Direct compact diff for %s found an invalid deleted row."),
                    *Name);
                if (MatchingDeletes->IsEmpty())
                {
                    DeletesByKey.Remove(PrimaryKey);
                }

                Diff.UpdateDeletes.Add(OldRow);
                Diff.UpdateInserts.Add(MakeShared<RowType>(MoveTemp(Insert.Row)));
                continue;
            }

            Diff.Inserts.Add(MakeShared<RowType>(MoveTemp(Insert.Row)));
        }

        for (TPair<KeyType, TArray<TSharedPtr<RowType>>>& DeletePair : DeletesByKey)
        {
            for (TSharedPtr<RowType>& DeletedRow : DeletePair.Value)
            {
                checkf(DeletedRow.IsValid(),
                    TEXT("Direct compact diff for %s retained an invalid deleted row."),
                    *Name);
                Diff.Deletes.Add(MoveTemp(DeletedRow));
            }
        }

        return Diff;
    }

public:

    /**
     *  Apply Inserts + Deletes to the specified table.
     *  Inserts: increment refCount, add new entry when needed.
     *  Deletes: decrement refCount, remove when it reaches 0.
     */
    FTableAppliedDiff<RowType> ApplyDiff(
        const FString& Name,
        TArray<FWithBsatn<RowType>>&& Inserts,
        TArray<FWithBsatn<RowType>>&& Deletes)
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
        checkf(ApplyMode != UE::SpacetimeDB::ETableCacheApplyMode::DirectNativeDiff,
            TEXT("DirectNativeDiff for table %s requires a generated compact uint64 primary-key trait."),
            *Name);

        struct FDeletedRow
        {
            TSharedPtr<RowType> Row;
            bool bMatchedInsert = false;
        };
        struct FInsertedRow
        {
            TArray<uint8> Key;
            TSharedPtr<RowType> Row;
        };

        auto RemoveFromIndices = [this](const TArray<uint8>& Key, const TSharedPtr<RowType>& Row)
        {
            checkf(Row.IsValid(), TEXT("Cannot remove invalid row from table indices."));
            for (auto& IndexPair : Table->UniqueIndices)
            {
                IndexPair.Value->RemoveRow(Row);
            }
            for (auto& IndexPair : Table->BTreeIndices)
            {
                if (!ShouldApplyRuntimeBTreeIndex(IndexPair.Key))
                {
                    continue;
                }
                IndexPair.Value->RemoveRow(Key, Row);
            }
        };

        auto AddToIndices = [this](const TArray<uint8>& Key, const TSharedPtr<RowType>& Row)
        {
            checkf(Row.IsValid(), TEXT("Cannot add invalid row to table indices."));
            for (auto& IndexPair : Table->UniqueIndices)
            {
                IndexPair.Value->AddRow(Row);
            }
            for (auto& IndexPair : Table->BTreeIndices)
            {
                if (!ShouldApplyRuntimeBTreeIndex(IndexPair.Key))
                {
                    continue;
                }
                IndexPair.Value->AddRow(Key, Row);
            }
        };

        FTableAppliedDiff<RowType> Diff;
        Diff.Inserts.Reserve(Inserts.Num());
        Diff.Deletes.Reserve(Deletes.Num());
        Diff.UpdateDeletes.Reserve(FMath::Min(Inserts.Num(), Deletes.Num()));
        Diff.UpdateInserts.Reserve(FMath::Min(Inserts.Num(), Deletes.Num()));

        TMap<TArray<uint8>, FDeletedRow> DeletedRows;
        DeletedRows.Reserve(Deletes.Num());
        TArray<FInsertedRow> InsertedRows;
        InsertedRows.Reserve(Inserts.Num());

        for (const FWithBsatn<RowType>& Delete : Deletes)
        {
            const TArray<uint8>& Key = Delete.Bsatn;
            FRowEntry<RowType>* Entry = Table->Entries.Find(Key);
            if (!Entry)
            {
                continue;
            }

            checkf(Entry->RefCount > 0, TEXT("Table cache row for %s has invalid refcount before delete."), *Name);
            checkf(Entry->Row.IsValid(), TEXT("Table cache row for %s is invalid before delete."), *Name);
            FDeletedRow& Deleted = DeletedRows.FindOrAdd(Key);
            if (!Deleted.Row.IsValid())
            {
                Deleted.Row = Entry->Row;
            }
            --Entry->RefCount;
        }

        for (FWithBsatn<RowType>& Insert : Inserts)
        {
            const TArray<uint8>& Key = Insert.Bsatn;
            FRowEntry<RowType>* ExistingEntry = Table->Entries.Find(Key);
            FDeletedRow* MatchingDelete = DeletedRows.Find(Key);
            if (ExistingEntry)
            {
                ++ExistingEntry->RefCount;
                if (MatchingDelete)
                {
                    MatchingDelete->bMatchedInsert = true;
                }
                continue;
            }

            TSharedPtr<RowType> NewRow = MakeShared<RowType>(MoveTemp(Insert.Row));
            Table->Entries.Add(Key, FRowEntry<RowType>{NewRow, 1});
            InsertedRows.Add(FInsertedRow{Key, NewRow});
            Diff.Inserts.Add(NewRow);
        }

        for (const TPair<TArray<uint8>, FDeletedRow>& DeletedPair : DeletedRows)
        {
            const TArray<uint8>& Key = DeletedPair.Key;
            const FDeletedRow& Deleted = DeletedPair.Value;
            if (Deleted.bMatchedInsert)
            {
                continue;
            }

            FRowEntry<RowType>* Entry = Table->Entries.Find(Key);
            if (!Entry || Entry->RefCount > 0)
            {
                continue;
            }

            RemoveFromIndices(Key, Deleted.Row);
            Diff.Deletes.Add(Deleted.Row);
            Table->Entries.Remove(Key);
        }

        for (const FInsertedRow& Inserted : InsertedRows)
        {
            AddToIndices(Inserted.Key, Inserted.Row);
        }

        return Diff;
    }
private:
    bool ShouldApplyDirectNativeDiff() const
    {
        return ApplyMode == UE::SpacetimeDB::ETableCacheApplyMode::DirectNativeDiff;
    }

    bool ShouldApplyRuntimeBTreeIndex(const FString& IndexName) const
    {
        checkf(!IndexName.IsEmpty(), TEXT("Cannot apply runtime B-Tree index policy for an empty index name."));
        if (const UE::SpacetimeDB::ERuntimeBTreeIndexApplyMode* RuntimeMode = RuntimeBTreeIndexApplyModes.Find(IndexName))
        {
            return *RuntimeMode == UE::SpacetimeDB::ERuntimeBTreeIndexApplyMode::Apply;
        }
        return UE::SpacetimeDB::TTableCachePolicy<RowType>::GetRuntimeBTreeIndexApplyMode(IndexName)
            == UE::SpacetimeDB::ERuntimeBTreeIndexApplyMode::Apply;
    }

    UE::SpacetimeDB::ETableCacheApplyMode ApplyMode = UE::SpacetimeDB::TTableCachePolicy<RowType>::ApplyMode;
    TMap<FString, UE::SpacetimeDB::ERuntimeBTreeIndexApplyMode> RuntimeBTreeIndexApplyModes;
};
