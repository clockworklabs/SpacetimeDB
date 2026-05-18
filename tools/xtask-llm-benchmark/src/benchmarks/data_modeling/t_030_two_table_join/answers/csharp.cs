using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Customer")]
    public partial struct Customer
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Name;
    }

    [Table(Accessor = "Order")]
    public partial struct Order
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public ulong CustomerId;
        public string Product;
        public uint Amount;
    }

    [Table(Accessor = "OrderDetail")]
    public partial struct OrderDetail
    {
        [PrimaryKey]
        public ulong OrderId;
        public string CustomerName;
        public string Product;
        public uint Amount;
    }

    [Reducer]
    public static void BuildOrderDetails(ReducerContext ctx)
    {
        foreach (var o in ctx.Db.Order.Iter())
        {
            if (ctx.Db.Customer.Id.Find(o.CustomerId) is Customer c)
            {
                ctx.Db.OrderDetail.Insert(new OrderDetail
                {
                    OrderId = o.Id,
                    CustomerName = c.Name,
                    Product = o.Product,
                    Amount = o.Amount,
                });
            }
        }
    }
}
