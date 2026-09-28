using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Sale", Public = true)]
    public partial struct Sale { [PrimaryKey] public ulong Id; public string Category; public long Amount; }

    [Table(Accessor = "CategoryTotal", Public = true)]
    public partial struct CategoryTotal { [PrimaryKey] public string Category; public long TotalAmount; public ulong SaleCount; }

    private static void AddToTotal(ReducerContext ctx, string category, long amount)
    {
        var total = ctx.Db.CategoryTotal.Category.Find(category);
        if (total is null) ctx.Db.CategoryTotal.Insert(new CategoryTotal { Category = category, TotalAmount = amount, SaleCount = 1 });
        else { var row = total.Value; row.TotalAmount += amount; row.SaleCount += 1; ctx.Db.CategoryTotal.Category.Update(row); }
    }

    private static void RemoveFromTotal(ReducerContext ctx, string category, long amount)
    {
        var row = ctx.Db.CategoryTotal.Category.Find(category) ?? throw new InvalidOperationException("missing category total");
        if (row.SaleCount == 1) ctx.Db.CategoryTotal.Category.Delete(category);
        else { row.TotalAmount -= amount; row.SaleCount -= 1; ctx.Db.CategoryTotal.Category.Update(row); }
    }

    private static void UpsertSale(ReducerContext ctx, Sale sale)
    {
        var old = ctx.Db.Sale.Id.Find(sale.Id);
        if (old is null) ctx.Db.Sale.Insert(sale);
        else { RemoveFromTotal(ctx, old.Value.Category, old.Value.Amount); ctx.Db.Sale.Id.Update(sale); }
        AddToTotal(ctx, sale.Category, sale.Amount);
    }

    private static void DeleteSale(ReducerContext ctx, ulong id)
    {
        var old = ctx.Db.Sale.Id.Find(id);
        if (old is null) return;
        ctx.Db.Sale.Id.Delete(id);
        RemoveFromTotal(ctx, old.Value.Category, old.Value.Amount);
    }

    [Reducer]
    public static void Exercise(ReducerContext ctx)
    {
        UpsertSale(ctx, new Sale { Id = 1, Category = "books", Amount = 10 });
        UpsertSale(ctx, new Sale { Id = 2, Category = "books", Amount = 20 });
        UpsertSale(ctx, new Sale { Id = 2, Category = "books", Amount = 25 });
        UpsertSale(ctx, new Sale { Id = 3, Category = "games", Amount = 40 });
        DeleteSale(ctx, 3);
        DeleteSale(ctx, 1);
    }

    [SpacetimeDB.View(Accessor = "CategorySummary", Public = true)]
    public static IQuery<CategoryTotal> CategorySummary(ViewContext ctx) => ctx.From.CategoryTotal();
}
