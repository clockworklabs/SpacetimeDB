using System;
using System.Linq;
using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE
public static partial class Module
{
    [Table(Accessor = "CacheEntry", Public = true)]
    public partial struct CacheEntry
    {
        [PrimaryKey, AutoInc]
        public ulong Id;
        public string Product;
        public string Language;
        public string Value;
        public long ExpiresAt;
    }

    [SpacetimeDB.Procedure]
    public static string FetchCached(
        ProcedureContext ctx,
        string product,
        string language,
        ulong ttlMs,
        string url
    )
    {
        if (product.Length == 0 || language.Length == 0 || ttlMs > 60000)
            return "invalid input";
        var now = ctx.Timestamp.MicrosecondsSinceUnixEpoch;
        if (ttlMs > 0)
        {
            var cached = ctx.WithTx(tx =>
                tx.Db.CacheEntry.Iter()
                    .Where(r => r.Product == product && r.Language == language && r.ExpiresAt > now)
                    .Select(r => (CacheEntry?)r)
                    .FirstOrDefault()
            );
            if (cached is CacheEntry hit)
                return hit.Value;
        }
        return ctx
            .Http.Get(url)
            .Match(
                response =>
                {
                    if (response.StatusCode != 200)
                        return "upstream error";
                    var value = response.Body.ToStringUtf8Lossy();
                    if (ttlMs > 0)
                        ctx.WithTx(tx =>
                        {
                            var old = tx
                                .Db.CacheEntry.Iter()
                                .Where(r => r.Product == product && r.Language == language)
                                .Select(r => (CacheEntry?)r)
                                .FirstOrDefault();
                            var next = new CacheEntry
                            {
                                Id = old?.Id ?? 0,
                                Product = product,
                                Language = language,
                                Value = value,
                                ExpiresAt =
                                    tx.Timestamp.MicrosecondsSinceUnixEpoch + (long)ttlMs * 1000,
                            };
                            if (old is null)
                                tx.Db.CacheEntry.Insert(next);
                            else
                                tx.Db.CacheEntry.Id.Update(next);
                            return 0;
                        });
                    return value;
                },
                _ => "upstream error"
            );
    }
}
