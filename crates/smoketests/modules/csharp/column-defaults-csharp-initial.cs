
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "defaults_test_table", Public = true)]
    public partial struct DefaultsTestTable
    {
        public uint id;
    }
}
