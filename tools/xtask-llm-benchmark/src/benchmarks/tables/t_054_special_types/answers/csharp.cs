using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "SpecialValue", Public = true)]
    public partial struct SpecialValue
    {
        [PrimaryKey] public ulong Id;
        public Uuid Uuid;
        public ConnectionId ConnectionId;
        public TimeDuration Duration;
        public U128 Unsigned128;
        public I128 Signed128;
        public U256 Unsigned256;
        public I256 Signed256;
    }
}
