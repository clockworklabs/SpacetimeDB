import BinaryReader from '../lib/binary_reader.ts';
import {
  ProductType,
  type AlgebraicTypeType,
  type Deserializer,
  type ProductTypeType,
} from '../lib/algebraic_type.ts';
import type { BsatnRowList } from './client_api/types.ts';

export type RowDecoderTypespace = {
  types: AlgebraicTypeType[];
};

/** Decode a BSATN row list using its row product schema and optional typespace. */
export function decodeBsatnRowList<Row = unknown>(
  rowList: BsatnRowList,
  rowType: ProductTypeType,
  typespace?: RowDecoderTypespace
): Row[] {
  const deserializeRow: Deserializer<Row> = ProductType.makeDeserializer(
    rowType,
    typespace
  );
  const rows: Row[] = [];

  forEachBsatnRow(rowList, deserializeRow, row => {
    rows.push(row);
  });

  return rows;
}

function getRowCount(rowList: BsatnRowList): number | undefined {
  if (rowList.sizeHint.tag === 'RowOffsets') {
    return rowList.sizeHint.value.length;
  }

  return rowList.sizeHint.value > 0
    ? Math.floor(rowList.rowsData.length / rowList.sizeHint.value)
    : undefined;
}

export function forEachBsatnRow<Row>(
  rowList: BsatnRowList,
  deserializeRow: Deserializer<Row>,
  visit: (row: Row, rowBytes: Uint8Array) => void,
  reader?: BinaryReader
): void {
  const { rowsData } = rowList;
  const rowReader = reader ?? new BinaryReader(rowsData);
  rowReader.reset(rowsData);
  const rowCount = getRowCount(rowList);

  for (
    let i = 0;
    rowCount === undefined ? rowReader.remaining > 0 : i < rowCount;
    i++
  ) {
    const startOffset = rowReader.offset;
    const row = deserializeRow(rowReader);
    visit(row, rowsData.subarray(startOffset, rowReader.offset));
  }
}
