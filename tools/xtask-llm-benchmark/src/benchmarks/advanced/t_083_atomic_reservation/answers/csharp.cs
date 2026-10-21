using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Stock", Public = true)]
    public partial struct Stock
    {
        [PrimaryKey]
        public ulong Id;
        public long Available;
    }

    [Table(Accessor = "Reservation", Public = true)]
    public partial struct Reservation
    {
        [PrimaryKey]
        public string RequestId;
        public ulong FirstId;
        public long FirstQty;
        public ulong SecondId;
        public long SecondQty;
    }

    [Reducer]
    public static void AddStock(ReducerContext ctx, ulong id, long available)
    {
        if (available < 0)
            throw new Exception("invalid stock");
        ctx.Db.Stock.Insert(new Stock { Id = id, Available = available });
    }

    [Reducer]
    public static void Reserve(
        ReducerContext ctx,
        string requestId,
        ulong firstId,
        long firstQty,
        ulong secondId,
        long secondQty
    )
    {
        if (ctx.Db.Reservation.RequestId.Find(requestId) is Reservation old)
        {
            if (
                old.FirstId != firstId
                || old.FirstQty != firstQty
                || old.SecondId != secondId
                || old.SecondQty != secondQty
            )
                throw new Exception("request conflict");
            return;
        }
        if (requestId.Length == 0 || firstId == secondId || firstQty <= 0 || secondQty <= 0)
            throw new Exception("invalid reservation");
        var first = ctx.Db.Stock.Id.Find(firstId) ?? throw new Exception("missing product");
        var second = ctx.Db.Stock.Id.Find(secondId) ?? throw new Exception("missing product");
        if (first.Available < firstQty || second.Available < secondQty)
            throw new Exception("insufficient stock");
        first.Available -= firstQty;
        second.Available -= secondQty;
        ctx.Db.Stock.Id.Update(first);
        ctx.Db.Stock.Id.Update(second);
        ctx.Db.Reservation.Insert(
            new Reservation
            {
                RequestId = requestId,
                FirstId = firstId,
                FirstQty = firstQty,
                SecondId = secondId,
                SecondQty = secondQty,
            }
        );
    }
}
