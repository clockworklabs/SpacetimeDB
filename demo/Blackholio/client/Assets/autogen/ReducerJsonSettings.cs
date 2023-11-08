using SpacetimeDB;

namespace SpacetimeDB.Types
{
	[ReducerClass]
	public partial class Reducer
	{
		private static Newtonsoft.Json.JsonSerializerSettings _settings = new Newtonsoft.Json.JsonSerializerSettings
		{
			Converters = { new SpacetimeDB.SomeWrapperConverter(), new SpacetimeDB.EnumWrapperConverter() },
			ContractResolver = new SpacetimeDB.JsonContractResolver(),
		};
	}
}
