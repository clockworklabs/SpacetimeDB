using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Sale", Public = true)]
    public partial struct Sale
    {
        [PrimaryKey]
        public ulong Id;
        public string Category;
        public long Amount;
    }

    [Table(Accessor = "CategoryTotal", Public = true)]
    public partial struct CategoryTotal
    {
        [PrimaryKey]
        public string Category;
        public long TotalAmount;
        public ulong SaleCount;
    }

    private static void Adjust(ReducerContext ctx, string category, long amount, bool adding)
    {
        var total =
            ctx.Db.CategoryTotal.Category.Find(category)
            ?? new CategoryTotal { Category = category };
        total.TotalAmount += adding ? amount : -amount;
        if (adding)
            total.SaleCount++;
        else
            total.SaleCount--;
        ctx.Db.CategoryTotal.Category.Delete(category);
        if (total.SaleCount > 0)
            ctx.Db.CategoryTotal.Insert(total);
    }

    [Reducer]
    public static void SetSale(ReducerContext ctx, ulong id, string category, long amount)
    {
        if (category.Length == 0)
            throw new Exception("invalid category");
        if (ctx.Db.Sale.Id.Find(id) is Sale old)
        {
            Adjust(ctx, old.Category, old.Amount, false);
            ctx.Db.Sale.Id.Delete(id);
        }
        ctx.Db.Sale.Insert(
            new Sale
            {
                Id = id,
                Category = category,
                Amount = amount,
            }
        );
        Adjust(ctx, category, amount, true);
    }

    [Reducer]
    public static void RemoveSale(ReducerContext ctx, ulong id)
    {
        if (ctx.Db.Sale.Id.Find(id) is Sale old)
        {
            ctx.Db.Sale.Id.Delete(id);
            Adjust(ctx, old.Category, old.Amount, false);
        }
    }
}
