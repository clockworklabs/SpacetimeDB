import { schema, table, t } from 'spacetimedb/server';

const blobRecord = table(
  {
    name: 'blob_record',
    public: true,
    indexes: [{ accessor: 'byOwner', algorithm: 'btree', columns: ['owner'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.identity(),
    filename: t.string(),
    mimeType: t.string(),
    size: t.u64(),
    data: t.array(t.u8()),
  }
);

const spacetimedb = schema({ blobRecord });
export default spacetimedb;

export const store_blob = spacetimedb.reducer(
  { filename: t.string(), mimeType: t.string(), data: t.array(t.u8()) },
  (ctx, { filename, mimeType, data }) => {
    ctx.db.blobRecord.insert({
      id: 0n,
      owner: ctx.sender,
      filename,
      mimeType,
      size: BigInt(data.length),
      data,
    });
  }
);
