using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "DamageEvent", Public = true, Event = true)]
    public partial struct DamageEvent
    {
        public ulong EntityId;
        public uint Damage;
        public string Source;
    }

    [Reducer]
    public static void DealDamage(ReducerContext ctx, ulong entityId, uint damage, string source)
    {
        ctx.Db.DamageEvent.Insert(new DamageEvent
        {
            EntityId = entityId,
            Damage = damage,
            Source = source,
        });
    }
}
