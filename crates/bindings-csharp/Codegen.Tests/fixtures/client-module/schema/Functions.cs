using System.Collections.Generic;

namespace Shared
{
    public static partial class Methods
    {
        [SpacetimeDB.Reducer]
        public static partial void Add(
            SpacetimeDB.ReducerContext ctx,
            string getHTTPCode,
            int HTTP_result,
            bool player2D
        );

        [SpacetimeDB.Procedure]
        public static partial string Greet(SpacetimeDB.ProcedureContext ctx);

        [SpacetimeDB.View(Name = "all", Public = true)]
        public static partial List<Person> All(SpacetimeDB.ViewContext ctx);
    }
}
