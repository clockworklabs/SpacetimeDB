using System;
using System.Linq;
using System.Collections.Generic;
using SpacetimeDB;
public static partial class Module
{
    [Table(Accessor = "PrivateNote")]
    public partial struct PrivateNote
    {
        [PrimaryKey] public ulong Id;
        [SpacetimeDB.Index.BTree] public Identity Owner;
        public string Tag;
        public string Text;
    }
    [SpacetimeDB.Type]
    public partial struct NoteResult { public ulong Id; public string Text; }
    [Reducer]
    public static void PutNote(ReducerContext ctx, ulong id, string tag, string text)
    {
        var old = ctx.Db.PrivateNote.Id.Find(id);
        if (old is PrivateNote existing && existing.Owner != ctx.Sender) throw new Exception("owner only");
        var row = new PrivateNote { Id = id, Owner = ctx.Sender, Tag = tag, Text = text };
        if (old is null) ctx.Db.PrivateNote.Insert(row); else ctx.Db.PrivateNote.Id.Update(row);
    }
    [Reducer]
    public static void RemoveNote(ReducerContext ctx, ulong id)
    {
        if (ctx.Db.PrivateNote.Id.Find(id) is PrivateNote old)
        {
            if (old.Owner != ctx.Sender) throw new Exception("owner only");
            ctx.Db.PrivateNote.Id.Delete(id);
        }
    }
    [View(Accessor = "NotesForTag", Public = true)]
    public static List<NoteResult> NotesForTag(ViewContext ctx, string tag) =>
        ctx.Db.PrivateNote.Owner.Filter(ctx.Sender).Where(row => row.Tag == tag)
            .Select(row => new NoteResult { Id = row.Id, Text = row.Text }).ToList();
}
