// The entry file of your WebAssembly module.
import * as stdb from "./stdb"

export function warmup(): void {};

export function reduce(actor: u64): void {
  stdb.createTable(0, [
    {colId: 0, colType: 3},
    {colId: 1, colType: 3},
    {colId: 2, colType: 3},
  ]);
  for (let i = 0; i < 100; i++) {
    stdb.insert(0, [
      {type: 3, value: i},
      {type: 3, value: 87},
      {type: 3, value: 33},
    ]);
  }
}
