#pragma once

#include "CoreMinimal.h"
#include "ModuleBindings/Types/TableUpdateType.g.h"
#include "ModuleBindings/Types/TableUpdateRowsType.g.h"
#include "DBCache/WithBsatn.h"

/** Helper utilities for working with BSATN encoded row data in Unreal. */

namespace UE::SpacetimeDB
{
	enum class EQueryRowsApplyMode : uint8
	{
		Inserts,
		Deletes
	};

	namespace Private
	{
		template<typename RowType>
		static void AddParsedRowWithBsatn(const uint8* RowData, int32 RowLength, TArray<FWithBsatn<RowType>>& OutRows)
		{
			checkf(RowLength >= 0, TEXT("Cannot parse a negative BSATN row length: %d"), RowLength);
			checkf(RowData != nullptr || RowLength == 0, TEXT("Cannot parse null BSATN row data with length %d"), RowLength);

			RowType Row = DeserializeView<RowType>(RowData, RowLength);
			TArray<uint8> Bsatn;
			if (RowLength > 0)
			{
				Bsatn.Append(RowData, RowLength);
			}
			OutRows.Add(FWithBsatn<RowType>(MoveTemp(Bsatn), MoveTemp(Row)));
		}
	}

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
			const uint16 Size = List.SizeHint.GetAsFixedSize();
			if (Size > 0)
			{
				checkf(List.RowsData.Num() % Size == 0,
					TEXT("Fixed-size BSATN row list has %d bytes, which is not divisible by row size %u"),
					List.RowsData.Num(),
					static_cast<uint32>(Size));
				// If the size is valid, parse the rows based on the fixed size
				const int32 Count = List.RowsData.Num() / Size;
				OutRows.Reserve(OutRows.Num() + Count);
				for (int32 i = 0; i < Count; ++i)
				{
					Private::AddParsedRowWithBsatn<RowType>(List.RowsData.GetData() + i * Size, Size, OutRows);
				}
				return;
			}
		}
		// If the size hint is row offsets, parse the rows based on the offsets
		else if (List.SizeHint.IsRowOffsets())
		{
			// Get the offsets from the size hint
			const TArray<uint64>& Offsets = List.SizeHint.MessageData.Get<TArray<uint64>>();
			if (Offsets.Num() > 0)
			{
				// If the offsets are valid, parse the rows based on the offsets
				OutRows.Reserve(OutRows.Num() + Offsets.Num());
				for (int32 i = 0; i < Offsets.Num(); ++i)
				{
					// If this is the last offset, read until the end of the data
					const uint64 Start = Offsets[i];
					const uint64 End = (i + 1 < Offsets.Num()) ? Offsets[i + 1] : static_cast<uint64>(List.RowsData.Num());
					checkf(Start <= End,
						TEXT("BSATN row offsets are not sorted: start=%llu end=%llu row_index=%d"),
						Start,
						End,
						i);
					checkf(End <= static_cast<uint64>(List.RowsData.Num()),
						TEXT("BSATN row offset %llu exceeds row data size %d at row_index=%d"),
						End,
						List.RowsData.Num(),
						i);
					const int32 RowStart = static_cast<int32>(Start);
					const int32 RowLength = static_cast<int32>(End - Start);
					Private::AddParsedRowWithBsatn<RowType>(List.RowsData.GetData() + RowStart, RowLength, OutRows);
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
				const FPersistentTableRowsType& Persistent = RowSet.MessageData.Get<FPersistentTableRowsType>();
				ParseRowListWithBsatn(Persistent.Inserts, Inserts);
				ParseRowListWithBsatn(Persistent.Deletes, Deletes);
			}
			// Event-table rows are callback-only inserts and should not create delete paths.
			else if (RowSet.IsEventTable())
			{
				const FEventTableRowsType& EventRows = RowSet.MessageData.Get<FEventTableRowsType>();
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
		virtual int64 EstimateMemoryBytes() const
		{
			return sizeof(FPreprocessedTableDataBase);
		}
		int32 InsertRowCount = 0;
		int32 DeleteRowCount = 0;
		int32 RowSetCount = 0;
		int64 InsertRowBytes = 0;
		int64 DeleteRowBytes = 0;
	};

	/** A wrapper for a row type that includes its bsatn value. Used to store rows with their bsatn values. */
	template<typename RowType>
	struct TPreprocessedTableData : FPreprocessedTableDataBase
	{
		// The type of the row being processed
		TArray<FWithBsatn<RowType>> Inserts;
		TArray<FWithBsatn<RowType>> Deletes;

		virtual int64 EstimateMemoryBytes() const override
		{
			auto EstimateRowsBytes = [](const TArray<FWithBsatn<RowType>>& Rows)
			{
				int64 Bytes = Rows.GetAllocatedSize();
				for (const FWithBsatn<RowType>& Row : Rows)
				{
					Bytes += Row.Bsatn.GetAllocatedSize();
				}
				return Bytes;
			};

			return sizeof(TPreprocessedTableData<RowType>)
				+ EstimateRowsBytes(Inserts)
				+ EstimateRowsBytes(Deletes);
		}
	};

	/** Interface for deserializing table rows from a database update. Allows for different row types to be processed in SDK. */
	class ITableRowDeserializer
	{
	public:
		virtual ~ITableRowDeserializer() {}
		/** Preprocess the table update and return a shared pointer to preprocessed data. */
		virtual TSharedPtr<FPreprocessedTableDataBase> PreProcess(const TArray<FTableUpdateRowsType>& RowSets, const FString& TableName) const = 0;
		virtual TSharedPtr<FPreprocessedTableDataBase> PreProcessQueryRows(const FBsatnRowListType& Rows, EQueryRowsApplyMode Mode, const FString& TableName) const = 0;
	};

	/** Specialization of ITableRowDeserializer for a specific row type not defined in SDK. Used to deserialize rows of a specific type from a database update. */
	template<typename RowType>
	class TTableRowDeserializer : public ITableRowDeserializer
	{
	public:
		virtual TSharedPtr<FPreprocessedTableDataBase> PreProcess(const TArray<FTableUpdateRowsType>& RowSets, const FString& TableName) const override
		{
			// Create a new preprocessed table data object for the specific row type
			TSharedPtr<TPreprocessedTableData<RowType>> Result = MakeShared<TPreprocessedTableData<RowType>>();
			Result->RowSetCount = RowSets.Num();
			// Process each row-set update in the table update
			for (const FTableUpdateRowsType& RowSet : RowSets)
			{
				if (RowSet.IsPersistentTable())
				{
					const FPersistentTableRowsType& Persistent = RowSet.MessageData.Get<FPersistentTableRowsType>();
					const int32 InsertCountBefore = Result->Inserts.Num();
					const int32 DeleteCountBefore = Result->Deletes.Num();
					ParseRowListWithBsatn<RowType>(Persistent.Inserts, Result->Inserts);
					ParseRowListWithBsatn<RowType>(Persistent.Deletes, Result->Deletes);
					Result->InsertRowCount += Result->Inserts.Num() - InsertCountBefore;
					Result->DeleteRowCount += Result->Deletes.Num() - DeleteCountBefore;
					Result->InsertRowBytes += Persistent.Inserts.RowsData.Num();
					Result->DeleteRowBytes += Persistent.Deletes.RowsData.Num();
				}
				else if (RowSet.IsEventTable())
				{
					// Event rows are insert-style callback payloads only.
					const FEventTableRowsType& Events = RowSet.MessageData.Get<FEventTableRowsType>();
					const int32 InsertCountBefore = Result->Inserts.Num();
					ParseRowListWithBsatn<RowType>(Events.Events, Result->Inserts);
					Result->InsertRowCount += Result->Inserts.Num() - InsertCountBefore;
					Result->InsertRowBytes += Events.Events.RowsData.Num();
				}
				else
				{
					UE_LOG(LogTemp, Warning, TEXT("Unknown row-set tag for table %s"), *TableName);
				}
			}
			return Result;
		}

		virtual TSharedPtr<FPreprocessedTableDataBase> PreProcessQueryRows(const FBsatnRowListType& Rows, EQueryRowsApplyMode Mode, const FString& TableName) const override
		{
			TSharedPtr<TPreprocessedTableData<RowType>> Result = MakeShared<TPreprocessedTableData<RowType>>();
			Result->RowSetCount = 1;
			switch (Mode)
			{
			case EQueryRowsApplyMode::Inserts:
			{
				const int32 InsertCountBefore = Result->Inserts.Num();
				ParseRowListWithBsatn<RowType>(Rows, Result->Inserts);
				Result->InsertRowCount += Result->Inserts.Num() - InsertCountBefore;
				Result->InsertRowBytes += Rows.RowsData.Num();
				break;
			}
			case EQueryRowsApplyMode::Deletes:
			{
				const int32 DeleteCountBefore = Result->Deletes.Num();
				ParseRowListWithBsatn<RowType>(Rows, Result->Deletes);
				Result->DeleteRowCount += Result->Deletes.Num() - DeleteCountBefore;
				Result->DeleteRowBytes += Rows.RowsData.Num();
				break;
			}
			default:
				checkf(false, TEXT("Unsupported query-row apply mode for table %s"), *TableName);
				break;
			}
			return Result;
		}
	};
}
