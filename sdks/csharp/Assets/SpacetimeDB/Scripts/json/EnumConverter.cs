using System;
using System.Collections.Generic;
using System.Linq;
using Newtonsoft.Json;

namespace SpacetimeDB
{
    public class EnumConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType) => true;

        public override object ReadJson(JsonReader reader, Type objectType, object existingValue,
            JsonSerializer serializer)
        {
            throw new NotImplementedException();
        }

        public override void WriteJson(JsonWriter writer, object value, JsonSerializer serializer)
        {
            writer.WriteStartObject();
            writer.WritePropertyName(value.ToString());
            writer.WriteRaw("{}");
            writer.WriteEndObject();
        }
    }
}