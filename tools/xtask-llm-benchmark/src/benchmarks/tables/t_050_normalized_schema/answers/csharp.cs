using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Organization", Public = true)]
    public partial struct Organization
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public string Name;
    }

    [Table(Accessor = "Department", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_organization", Columns = new[] { nameof(OrganizationId) })]
    public partial struct Department
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public ulong OrganizationId;
        public string Name;
    }

    [Table(Accessor = "Employee", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_department", Columns = new[] { nameof(DepartmentId) })]
    public partial struct Employee
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public ulong DepartmentId;
        public string Name;
    }
}
