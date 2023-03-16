using System;
using System.Collections.Generic;
using System.Linq;
using Newtonsoft.Json;

namespace SpacetimeDB
{
    public class SpacetimeDBConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType) => ignoreValues.Where(value => value != null).All(value => value.GetType() != objectType);

        private readonly List<object> someValues = new List<object>();
        private readonly List<object> enumValues = new List<object>();
        private readonly List<object> ignoreValues = new List<object>();

        public void AddSomeValue(object o) => someValues.Add(o);
        public void AddEnumValue(object o) => enumValues.Add(o);
        public void AddIgnore(object o) => ignoreValues.Add(o);

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

            if (someValues.Contains(value))
            {
                writer.WriteStartObject();
                writer.WritePropertyName("some");
                internalSerializer.Serialize(writer, value);
                writer.WriteEndObject();
            } else if (enumValues.Contains(value))
            {
                writer.WriteStartObject();
                writer.WritePropertyName(value.ToString());
                writer.WriteRaw("{}");
                writer.WriteEndObject();
            } else
            {
                internalSerializer.Serialize(writer, value);
            }
        }
    }
}