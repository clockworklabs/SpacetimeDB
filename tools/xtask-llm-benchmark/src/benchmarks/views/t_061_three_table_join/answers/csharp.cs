using SpacetimeDB;
public static partial class Module
{
    [Table(Accessor = "Customer", Public = true)] public partial struct Customer { [PrimaryKey] public ulong Id; public string Name; }
    [Table(Accessor = "Purchase", Public = true)] public partial struct Purchase { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public ulong CustomerId; }
    [Table(Accessor = "LineItem", Public = true)] public partial struct LineItem { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public ulong PurchaseId; public string Sku; [SpacetimeDB.Index.BTree] public bool Visible; }
    [SpacetimeDB.Type] public partial struct OrderLineDetail { public ulong LineId; public string CustomerName; public string Sku; }
    [Reducer] public static void Seed(ReducerContext ctx) { ctx.Db.Customer.Insert(new Customer { Id = 1, Name = "Ada" }); ctx.Db.Purchase.Insert(new Purchase { Id = 10, CustomerId = 1 }); ctx.Db.LineItem.Insert(new LineItem { Id = 100, PurchaseId = 10, Sku = "SKU-1", Visible = true }); }
    [SpacetimeDB.View(Accessor = "OrderLineDetail", Public = true)]
    public static IEnumerable<OrderLineDetail> OrderLineDetails(AnonymousViewContext ctx)
    {
        foreach (var line in ctx.Db.LineItem.Visible.Filter(true)) {
            var purchase = ctx.Db.Purchase.Id.Find(line.PurchaseId); if (purchase is null) continue;
            var customer = ctx.Db.Customer.Id.Find(purchase.Value.CustomerId); if (customer is null) continue;
            yield return new OrderLineDetail { LineId = line.Id, CustomerName = customer.Value.Name, Sku = line.Sku };
        }
    }
}
