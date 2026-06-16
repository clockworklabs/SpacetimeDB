type Row = AlgebraicValue;

enum RowOps {
    Insert(TableId, Row),
    Delete(TableId, Row),
    Update(TableId, Row, Row),
}
