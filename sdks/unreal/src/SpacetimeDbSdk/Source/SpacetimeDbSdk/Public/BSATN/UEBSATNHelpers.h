#pragma once

#include "CoreMinimal.h"
#include "ModuleBindings/Types/TableUpdateType.g.h"
#include "ModuleBindings/Types/TableUpdateRowsType.g.h"
#include "DBCache/WithBsatn.h"

/** Helper utilities for working with BSATN encoded row data in Unreal. */

namespace UE::SpacetimeDB
{

	/** Parse a single row list based on its size hint and retain BSATN bytes */
	template<typename RowType>
	static void ParseRowListWithBsatn(
		const FBsatnRowListType& List,
		TArray<FWithBsatn<RowType>>& OutRows
	)
	{
		// If the size hint is fixed size, parse the rows based on the fixed size
		if (List.SizeHint.IsFixedSize())
		{
			// Get the fixed size from the size hint
			uint16 Size = List.SizeHint.GetAsFixedSize();
			if (Size > 0)
			{
				// If the size is valid, parse the rows based on the fixed size
				int32 Count = List.RowsData.Num() / Size;
				for (int32 i = 0; i < Count; ++i)
				{
					// Create a slice of the row data based on the fixed size
					TArray<uint8> Slice;
					Slice.Append(List.RowsData.GetData() + i * Size, Size);
					// Deserialize the row from the slice
					RowType Row = UE::SpacetimeDB::Deserialize<RowType>(Slice);
					// Add the row with its BSATN bytes to the output array
					OutRows.Add(FWithBsatn<RowType>(Slice, Row));
				}
				return;
			}
		}
		// If the size hint is row offsets, parse the rows based on the offsets
		else if (List.SizeHint.IsRowOffsets())
		{
			// Get the offsets from the size hint
			TArray<uint64> Offsets = List.SizeHint.GetAsRowOffsets();
			if (Offsets.Num() > 0)
			{
				// If the offsets are valid, parse the rows based on the offsets
				UEReader Reader(List.RowsData);
				for (int32 i = 0; i < Offsets.Num(); ++i)
				{
					// If this is the last offset, read until the end of the data
					int64 Start = Offsets[i];
					int64 End = (i + 1 < Offsets.Num()) ? Offsets[i + 1] : List.RowsData.Num();
					int64 Length = End - Start;
					TArray<uint8> Slice;
					Slice.Append(List.RowsData.GetData() + Start, Length);

					// Deserialize the row from the slice
					UEReader SliceReader(Slice);
					RowType Row = deserialize<RowType>(SliceReader);

					// Add the row with its BSATN bytes to the output array
					OutRows.Add(FWithBsatn<RowType>(Slice, Row));
				}
			}
		}
	}

	/** Apply a table update keeping BSATN bytes */
	template<typename RowType>
	void ProcessTableUpdateWithBsatn(
		const FTableUpdateType& TableUpdate,
		TArray<FWithBsatn<RowType>>& Inserts,
		TArray<FWithBsatn<RowType>>& Deletes)
	{
		for (const FTableUpdateRowsType& RowSet : TableUpdate.Rows)
		{
			if (RowSet.IsPersistentTable())
			{
				const FPersistentTableRowsType Persistent = RowSet.GetAsPersistentTable();
				ParseRowListWithBsatn(Persistent.Inserts, Inserts);
				ParseRowListWithBsatn(Persistent.Deletes, Deletes);
			}
			// Event-table rows are callback-only inserts and should not create delete paths.
			else if (RowSet.IsEventTable())
			{
				const FEventTableRowsType EventRows = RowSet.GetAsEventTable();
				ParseRowListWithBsatn(EventRows.Events, Inserts);
			}
			else
			{
				UE_LOG(LogTemp, Warning, TEXT("Unknown row-set tag for table %s"), *TableUpdate.TableName);
			}
		}
	}

	/** Base class for preprocessed table data. Used to store inserts and deletes for a specific row type. */
	struct FPreprocessedTableDataBase
	{
		virtual ~FPreprocessedTableDataBase() {}
	};

	/** A wrapper for a row type that includes its bsatn value. Used to store rows with their bsatn values. */
	template<typename RowType>
	struct TPreprocessedTableData : FPreprocessedTableDataBase
	{
		// The type of the row being processed
		TArray<FWithBsatn<RowType>> Inserts;
		TArray<FWithBsatn<RowType>> Deletes;
	};

	/** Interface for deserializing table rows from a database update. Allows for different row types to be processed in SDK. */
	class ITableRowDeserializer
	{
	public:
		virtual ~ITableRowDeserializer() {}
		/** Preprocess the table update and return a shared pointer to preprocessed data. */
		virtual TSharedPtr<FPreprocessedTableDataBase> PreProcess(const TArray<FTableUpdateRowsType>& RowSets, const FString TableName) const = 0;
	};

	/** Specialization of ITableRowDeserializer for a specific row type not defined in SDK. Used to deserialize rows of a specific type from a database update. */
	template<typename RowType>
	class TTableRowDeserializer : public ITableRowDeserializer
	{
	public:
		virtual TSharedPtr<FPreprocessedTableDataBase> PreProcess(const TArray<FTableUpdateRowsType>& RowSets, const FString TableName) const override
		{
			// Create a new preprocessed table data object for the specific row type
			TSharedPtr<TPreprocessedTableData<RowType>> Result = MakeShared<TPreprocessedTableData<RowType>>();
			// Process each row-set update in the table update
			for (const FTableUpdateRowsType& RowSet : RowSets)
			{
				if (RowSet.IsPersistentTable())
				{
					const FPersistentTableRowsType Persistent = RowSet.GetAsPersistentTable();
					ParseRowListWithBsatn<RowType>(Persistent.Inserts, Result->Inserts);
					ParseRowListWithBsatn<RowType>(Persistent.Deletes, Result->Deletes);
				}
				else if (RowSet.IsEventTable())
				{
					// Event rows are insert-style callback payloads only.
					const FEventTableRowsType Events = RowSet.GetAsEventTable();
					ParseRowListWithBsatn<RowType>(Events.Events, Result->Inserts);
				}
				else
				{
					UE_LOG(LogTemp, Warning, TEXT("Unknown row-set tag for table %s"), *TableName);
				}
			}
			return Result;
		}
	};
}
