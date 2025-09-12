import { useEffect, useState } from "react";

export interface TableCallbacks<TRow> {
  onInsert?: (row: TRow) => void;
  onDelete?: (row: TRow) => void;
  onUpdate?: (oldRow: TRow, newRow: TRow) => void;
}

export function useTable<TRow>(table: any, callbacks?: TableCallbacks<TRow>): TRow[] {
  const [rows, setRows] = useState<TRow[]>([]);

  useEffect(() => {
    table.onInsert((_: any, row: any) => {
      setRows(table.iter() as TRow[]);
      if (callbacks?.onInsert) {
        callbacks.onInsert(row);
      }
    });

    table.onDelete((_: any, row: any) => {
      setRows(table.iter() as TRow[]);
      if (callbacks?.onDelete) {
        callbacks.onDelete(row);
      }
    });

    if (table.onUpdate) {
      table.onUpdate((_: any, oldRow: any, newRow: any) => {
        setRows(table.iter() as TRow[]);
        if (callbacks?.onUpdate) {
          callbacks.onUpdate(oldRow, newRow);
        }
      });
    }

    return () => {};
  }, [table, callbacks]);

  return rows;
}

// TODO:
// Add a hook for reducers
