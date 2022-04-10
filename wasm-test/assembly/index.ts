// The entry file of your WebAssembly module.
import * as stdb from "./stdb"

export function reduce(actor: u64): void {
  console.log("HAP0");
  stdb.createTable(0, [
    {colId: 0, colType: 3},
    {colId: 1, colType: 3}
  ]);
  console.log("HAP1");
  stdb.insert(0, [
    {type: 3, value: 57},
    {type: 3, value: 87},
  ]);
}
