using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "LegacyItem", Public = true)]
    public partial struct LegacyItem
    {
        [PrimaryKey]
        public ulong Id;
        public string Value;
        public ulong Revision;
        public bool Deleted;
    }

    [Table(Accessor = "BackfillSnapshot", Public = true)]
    public partial struct BackfillSnapshot
    {
        [PrimaryKey]
        public ulong Id;
        public string Value;
        public ulong Revision;
        public bool Deleted;
    }

    [Table(Accessor = "ItemV2", Public = true)]
    public partial struct ItemV2
    {
        [PrimaryKey]
        public ulong Id;
        public string Value;
        public ulong Revision;
    }

    [Reducer]
    public static void WriteItem(ReducerContext ctx, ulong id, string value)
    {
        var revision = (ctx.Db.LegacyItem.Id.Find(id)?.Revision ?? 0) + 1;
        var row = new LegacyItem
        {
            Id = id,
            Value = value,
            Revision = revision,
            Deleted = false,
        };
        if (ctx.Db.LegacyItem.Id.Find(id) is null)
            ctx.Db.LegacyItem.Insert(row);
        else
            ctx.Db.LegacyItem.Id.Update(row);
        var next = new ItemV2
        {
            Id = id,
            Value = value,
            Revision = revision,
        };
        if (ctx.Db.ItemV2.Id.Find(id) is null)
            ctx.Db.ItemV2.Insert(next);
        else
            ctx.Db.ItemV2.Id.Update(next);
    }

    [Reducer]
    public static void SeedLegacy(ReducerContext ctx, ulong id, string value)
    {
        ctx.Db.LegacyItem.Insert(
            new LegacyItem
            {
                Id = id,
                Value = value,
                Revision = 1,
                Deleted = false,
            }
        );
    }

    [Reducer]
    public static void DeleteItem(ReducerContext ctx, ulong id)
    {
        if (ctx.Db.LegacyItem.Id.Find(id) is LegacyItem row && !row.Deleted)
        {
            row.Revision++;
            row.Deleted = true;
            ctx.Db.LegacyItem.Id.Update(row);
            ctx.Db.ItemV2.Id.Delete(id);
        }
    }

    [Reducer]
    public static void CaptureBatch(ReducerContext ctx, ulong afterId, ulong limit)
    {
        if (limit == 0 || limit > 10)
            throw new Exception("invalid limit");
        var rows = ctx
            .Db.LegacyItem.Iter()
            .Where(r => r.Id > afterId)
            .OrderBy(r => r.Id)
            .Take((int)limit)
            .ToArray();
        foreach (var row in ctx.Db.BackfillSnapshot.Iter().ToArray())
            ctx.Db.BackfillSnapshot.Id.Delete(row.Id);
        foreach (var r in rows)
            ctx.Db.BackfillSnapshot.Insert(
                new BackfillSnapshot
                {
                    Id = r.Id,
                    Value = r.Value,
                    Revision = r.Revision,
                    Deleted = r.Deleted,
                }
            );
    }

    [Reducer]
    public static void ApplyBatch(ReducerContext ctx)
    {
        foreach (var snapshot in ctx.Db.BackfillSnapshot.Iter())
        {
            var r = ctx.Db.LegacyItem.Id.Find(snapshot.Id) ?? throw new Exception("missing source");
            if (r.Deleted)
                ctx.Db.ItemV2.Id.Delete(r.Id);
            else
            {
                var next = new ItemV2
                {
                    Id = r.Id,
                    Value = r.Value,
                    Revision = r.Revision,
                };
                if (ctx.Db.ItemV2.Id.Find(r.Id) is null)
                    ctx.Db.ItemV2.Insert(next);
                else
                    ctx.Db.ItemV2.Id.Update(next);
            }
        }
    }
}
