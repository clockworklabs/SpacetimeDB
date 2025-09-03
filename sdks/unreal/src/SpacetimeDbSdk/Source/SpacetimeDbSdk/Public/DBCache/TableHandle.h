#pragma once
#include "CoreMinimal.h"
#include "ClientCache.h"

/* ============================================================================ *
 *  TableHandle.h (2025-05-28)
 *  ----------------------------------------------------------------------------
 *  Lightweight façade giving gameplay code easy, read‑only access to a table
 *  inside UClientCache without exposing internal maps.
 * ============================================================================ */
template<typename RowType>
class FTableHandle
{
public:
    TSharedPtr<UClientCache<RowType>> Cache;
    FString TableName;

    FTableHandle(TSharedPtr<UClientCache<RowType>> InCache, const FString& InName)
        : Cache(InCache), TableName(InName) 
    {
        bValid = true;
        if (!InCache.IsValid())
        {
            UE_LOG(LogTemp, Error, TEXT("FTableHandle: Invalid table data for '%s'"), *InName);
            bValid = false;
        }

        if (InName.IsEmpty())
        {
            UE_LOG(LogTemp, Warning, TEXT("FTableHandle created with empty name."));
            bValid = false;
        }
    }

    bool IsValid() const
    {
        return bValid;
    }

    /** O(1) row count. */
    int64 Count() const
    {
        auto T = Cache->GetTable(TableName);
        return T.IsValid() ? T->Entries.Num() : 0;
    }

    /** Copy all rows into an array. */
    TArray<RowType> GetAllRows() const
    {
        TArray<RowType> Out;

        auto T = Cache->GetTable(TableName);
        UE_LOG(LogTemp, Warning,
            TEXT("GetTable(%s) valid=%d"),
            *TableName, T.IsValid());

        if (T.IsValid())
        {
            for (const auto& Pair : T->Entries)
            {
                // Dereference the shared pointer to get the RowType value
                Out.Add(*Pair.Value.Row);
            }
        }
        return Out;
    }

    /** Blueprint friendly alias for iterating over all rows. */
    TArray<RowType> Iter() const
    {
        return GetAllRows();
    }

    /** Find a row via a unique index. */
    const RowType* FindUnique(const FString& IndexName, const void* Key) const
    {
        auto T = Cache->GetTable(TableName);
        return T.IsValid() ? T->FindByUniqueIndex(IndexName, Key) : nullptr;
    }

private:
    bool bValid;
};

/** Helper functionss for testing.Can be removed on production state. */

/** Templated functions to get all rows from table by name */
template<typename TData>
TArray<TData> GetAllRowsFromTable(TSharedPtr<UClientCache<TData>> Cache, const FString& TableName)
{
    TArray<TData> Result;
    FTableHandle<TData> Handle(Cache, TableName);
    if (Handle.IsValid())
    {
        Result = Handle.GetAllRows();
    }
    return Result;
}

/** Templated functions to get row count from table by name */
template<typename TData>
int32 GetRowCountFromTable(TSharedPtr<UClientCache<TData>> Cache, const FString& TableName)
{
    FTableHandle<TData> Handle(Cache, TableName);
    if (Handle.IsValid())
    {
        return Handle.Count();
    }
    return 0;
}

/** Template class for unique indexes */
template <typename TRowType, typename TKeyType, typename TTableCacheType>
class FUniqueIndexHelper
{
public:
    TSharedPtr<const TTableCacheType> Cache; // Table cache (can be assigned externally)
    FString UniqueIndexName; // The name of the unique index

    // Constructor to set the index name
    FUniqueIndexHelper(FString InUniqueIndexName = "")
        : UniqueIndexName(InUniqueIndexName)
    {
    }

    TRowType FindUniqueIndex(TKeyType Key)
    {
        if (Cache != nullptr)
        {
            check(Cache->UniqueIndices.Contains(UniqueIndexName));
            {
                const TRowType* Data = Cache->FindByUniqueIndex(UniqueIndexName, Key);
                if (Data != nullptr)
                {
                    TRowType ResultData = *Data;
                    return ResultData;
                }
            }
        }
        return TRowType(); // Return a default-constructed object if not found
    }
};
