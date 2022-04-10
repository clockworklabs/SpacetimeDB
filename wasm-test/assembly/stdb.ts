declare function _createTable(table_id: u32): void;
declare function _insert(table_id: u32): void;

memory.grow(1);

export class Column {
    colType: u32;
    colId: u32;
}

export function createTable(tableId: u32, columns: Array<Column>): void {
    let ptr = 0;
    for (let i = 0; i < columns.length; ++i) {
        let col = columns[i]
        store<u32>(ptr, col.colType);
        ptr += 4;
        store<u32>(ptr, col.colId);
        ptr += 4;
    }
    _createTable(tableId);
}

class ColValue {
    type: u8;
    value: u64;
}

export function insert(table_id: u32, colValues: Array<ColValue>): void {
    let ptr = 0;
    for (let i = 0; i < colValues.length; ++i) {
        const v = colValues[i];
        switch (v.type) {
            case 1:
                store<u8>(ptr, v.value);
                ptr += 1;
                break;
            case 2:
                store<u16>(ptr, v.value);
                ptr += 2;
                break;
            case 3:
                store<u32>(ptr, v.value);
                ptr += 4;
                break;
            case 4:
                store<u64>(ptr, v.value);
                ptr += 8;
                break;
            case 6:
                store<i8>(ptr, v.value);
                ptr += 1;
                break;
            case 7:
                store<i16>(ptr, v.value);
                ptr += 2;
                break;
            case 8:
                store<i32>(ptr, v.value);
                ptr += 4;
                break;
            case 9:
                store<i64>(ptr, v.value);
                ptr += 8;
                break;
        }
    }
    _insert(table_id);
}