using System;
using System.Collections.Generic;
using System.Linq;
using Newtonsoft.Json;

namespace SpacetimeDB
{
    public class SomeConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType) => true;

        public override object ReadJson(JsonReader reader, Type objectType, object existingValue,
            JsonSerializer serializer)
        {
            throw new NotImplementedException();
        }

        public override void WriteJson(JsonWriter writer, object value, JsonSerializer serializer)
        {
            var internalSerializer = new JsonSerializer
            {
                ContractResolver = serializer.ContractResolver,
                DateFormatHandling = serializer.DateFormatHandling,
                // Add any other settings you need from the original serializer
            };
            
            writer.WriteStartObject();
            writer.WritePropertyName("some");
            internalSerializer.Serialize(writer, value);
            writer.WriteEndObject();
        }
    }
}