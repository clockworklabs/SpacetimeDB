using System;
using Namespace;
using Newtonsoft.Json;

namespace SpacetimeDB
{
    public class SomeWrapperConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType) => objectType == typeof(SomeWrapper<>);

        public override object ReadJson(
            JsonReader reader,
            Type objectType,
            object existingValue,
            JsonSerializer serializer
        )
        {
            throw new NotImplementedException();
        }

        public override void WriteJson(JsonWriter writer, object value, JsonSerializer serializer)
        {
            writer.WriteStartObject();
            writer.WritePropertyName("some");
            serializer.Serialize(writer, value);
            writer.WriteEndObject();
        }
    }
}