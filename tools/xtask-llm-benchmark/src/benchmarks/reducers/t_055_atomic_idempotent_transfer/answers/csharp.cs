using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Account", Public = true)]
    public partial struct Account { [PrimaryKey] public ulong Id; public long Balance; }

    [Table(Accessor = "TransferRequest", Public = true)]
    public partial struct TransferRequest
    {
        [PrimaryKey] public string RequestId;
        public ulong FromId;
        public ulong ToId;
        public long Amount;
    }

    [Reducer]
    public static void CreateAccount(ReducerContext ctx, ulong id, long balance) =>
        ctx.Db.Account.Insert(new Account { Id = id, Balance = balance });

    [Reducer]
    public static void Transfer(ReducerContext ctx, string requestId, ulong fromId, ulong toId, long amount)
    {
        if (ctx.Db.TransferRequest.RequestId.Find(requestId) is not null) return;
        if (amount <= 0 || fromId == toId) throw new Exception("invalid transfer");
        var source = ctx.Db.Account.Id.Find(fromId) ?? throw new Exception("source account not found");
        var to = ctx.Db.Account.Id.Find(toId) ?? throw new Exception("destination account not found");
        if (source.Balance < amount) throw new Exception("insufficient balance");
        ctx.Db.Account.Id.Update(source with { Balance = source.Balance - amount });
        ctx.Db.Account.Id.Update(to with { Balance = to.Balance + amount });
        ctx.Db.TransferRequest.Insert(new TransferRequest { RequestId = requestId, FromId = fromId, ToId = toId, Amount = amount });
    }
}
