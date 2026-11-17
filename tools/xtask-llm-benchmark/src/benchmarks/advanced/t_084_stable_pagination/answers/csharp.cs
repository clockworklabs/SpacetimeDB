using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "AuditEntry", Public = true)]
    public partial struct AuditEntry
    {
        [PrimaryKey]
        public ulong Id;
        public string Tenant;
        public ulong OccurredAt;
        public bool Visible;
    }

    [Table(Accessor = "PageEntry", Public = true)]
    public partial struct PageEntry
    {
        [PrimaryKey]
        public ulong Position;
        public ulong EntryId;
        public ulong OccurredAt;
    }

    [Reducer]
    public static void AddEntry(
        ReducerContext ctx,
        ulong id,
        string tenant,
        ulong occurredAt,
        bool visible
    )
    {
        ctx.Db.AuditEntry.Insert(
            new AuditEntry
            {
                Id = id,
                Tenant = tenant,
                OccurredAt = occurredAt,
                Visible = visible,
            }
        );
    }

    [Reducer]
    public static void ReadPage(
        ReducerContext ctx,
        string tenant,
        ulong afterTime,
        ulong afterId,
        ulong limit
    )
    {
        if (limit == 0 || limit > 10)
            throw new Exception("invalid limit");
        var rows = ctx
            .Db.AuditEntry.Iter()
            .Where(r =>
                r.Tenant == tenant
                && r.Visible
                && (r.OccurredAt > afterTime || (r.OccurredAt == afterTime && r.Id > afterId))
            )
            .OrderBy(r => r.OccurredAt)
            .ThenBy(r => r.Id)
            .Take((int)limit)
            .ToArray();
        foreach (var old in ctx.Db.PageEntry.Iter().ToArray())
            ctx.Db.PageEntry.Position.Delete(old.Position);
        for (var i = 0; i < rows.Length; i++)
            ctx.Db.PageEntry.Insert(
                new PageEntry
                {
                    Position = (ulong)i,
                    EntryId = rows[i].Id,
                    OccurredAt = rows[i].OccurredAt,
                }
            );
    }
}
