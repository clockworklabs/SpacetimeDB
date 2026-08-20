using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "BlobRecord", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_owner", Columns = new[] { nameof(Owner) })]
    public partial struct BlobRecord
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public Identity Owner;
        public string Filename;
        public string MimeType;
        public ulong Size;
        public List<byte> Data;
    }

    [Reducer]
    public static void StoreBlob(ReducerContext ctx, string filename, string mimeType, List<byte> data)
    {
        ctx.Db.BlobRecord.Insert(new BlobRecord
        {
            Id = 0,
            Owner = ctx.Sender,
            Filename = filename,
            MimeType = mimeType,
            Size = (ulong)data.Count,
            Data = data,
        });
    }
}
