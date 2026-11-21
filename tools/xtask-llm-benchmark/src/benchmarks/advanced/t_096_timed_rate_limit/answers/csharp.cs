using System;
using System.Linq;
using SpacetimeDB;
public static partial class Module
{
    [Table(Accessor = "SearchLog", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "ByRequest", Columns = new[] { nameof(Owner), nameof(RequestId) })]
    public partial struct SearchLog {
        [PrimaryKey, AutoInc] public ulong Id;
        public Identity Owner; public string RequestId; public string Term;
    }
    [Table(Accessor = "SearchBucket")]
    public partial struct SearchBucket {
        [PrimaryKey] public Identity Owner; public uint Tokens; public Timestamp RefilledAt;
    }
    [Reducer]
    public static void LogSearch(ReducerContext ctx, string requestId, string term)
    {
        if (term.Length == 0) throw new Exception("empty term");
        foreach (var previous in ctx.Db.SearchLog.ByRequest.Filter((ctx.Sender, requestId))) {
            if (previous.Term != term) throw new Exception("request conflict");
            return;
        }
        var old = ctx.Db.SearchBucket.Owner.Find(ctx.Sender);
        var bucket = old ?? new SearchBucket { Owner = ctx.Sender, Tokens = 3, RefilledAt = ctx.Timestamp };
        var intervals = (ctx.Timestamp.MicrosecondsSinceUnixEpoch - bucket.RefilledAt.MicrosecondsSinceUnixEpoch) / 5_000_000;
        if (intervals > 0) {
            bucket.Tokens = (uint)Math.Min(3, bucket.Tokens + intervals);
            bucket.RefilledAt = new Timestamp(bucket.RefilledAt.MicrosecondsSinceUnixEpoch + intervals * 5_000_000);
        }
        if (bucket.Tokens == 0) throw new Exception("rate limited");
        bucket.Tokens--;
        if (old is null) ctx.Db.SearchBucket.Insert(bucket); else ctx.Db.SearchBucket.Owner.Update(bucket);
        ctx.Db.SearchLog.Insert(new SearchLog { Owner = ctx.Sender, RequestId = requestId, Term = term });
    }
}
