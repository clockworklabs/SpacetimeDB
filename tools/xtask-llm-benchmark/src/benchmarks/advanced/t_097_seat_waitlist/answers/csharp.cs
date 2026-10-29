using System;
using System.Linq;
using SpacetimeDB;
public static partial class Module
{
    [Table(Accessor = "SeatEvent", Public = true)]
    public partial struct SeatEvent { [PrimaryKey] public ulong Id; public uint Capacity; }
    [Table(Accessor = "Booking", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "ByOwnerEvent", Columns = new[] { nameof(Owner), nameof(EventId) })]
    public partial struct Booking {
        [PrimaryKey, AutoInc] public ulong Id; public Identity Owner;
        [SpacetimeDB.Index.BTree] public ulong EventId; public string Status;
    }
    [Reducer]
    public static void CreateEvent(ReducerContext ctx, ulong id, uint capacity)
    {
        if (capacity == 0) throw new Exception("invalid capacity");
        if (ctx.Db.SeatEvent.Id.Find(id) is not null) throw new Exception("event exists");
        ctx.Db.SeatEvent.Insert(new SeatEvent { Id = id, Capacity = capacity });
    }
    [Reducer]
    public static void JoinEvent(ReducerContext ctx, ulong eventId)
    {
        var item = ctx.Db.SeatEvent.Id.Find(eventId) ?? throw new Exception("missing event");
        if (ctx.Db.Booking.ByOwnerEvent.Filter((ctx.Sender, eventId)).Any(b => b.Status != "cancelled")) return;
        var seated = ctx.Db.Booking.EventId.Filter(eventId).Count(b => b.Status == "seated");
        ctx.Db.Booking.Insert(new Booking { Owner = ctx.Sender, EventId = eventId, Status = seated < item.Capacity ? "seated" : "waiting" });
    }
    [Reducer]
    public static void CancelBooking(ReducerContext ctx, ulong id)
    {
        if (ctx.Db.Booking.Id.Find(id) is not Booking row) return;
        if (row.Owner != ctx.Sender) throw new Exception("owner only");
        if (row.Status == "cancelled") return;
        var promote = row.Status == "seated";
        row.Status = "cancelled";
        ctx.Db.Booking.Id.Update(row);
        if (promote) {
            foreach (var next in ctx.Db.Booking.EventId.Filter(row.EventId).Where(b => b.Status == "waiting").OrderBy(b => b.Id).Take(1).ToList()) {
                var updated = next;
                updated.Status = "seated";
                ctx.Db.Booking.Id.Update(updated);
            }
        }
    }
}
