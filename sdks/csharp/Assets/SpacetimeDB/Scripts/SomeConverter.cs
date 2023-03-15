using System;
using System.Collections.Generic;
using Newtonsoft.Json;

namespace SpacetimeDB
{
    public class SomeConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType) => true;

        private readonly List<object> convertValues = new List<object>();

        public void Add(object o) => convertValues.Add(o);
        
        public override object ReadJson(JsonReader reader, Type objectType, object existingValue, JsonSerializer serializer)
        {
            throw new NotImplementedException();
        }

        public override void WriteJson(JsonWriter writer, object value, JsonSerializer serializer)
        {
            if (convertValues.Contains(value))
            {
                writer.WriteStartObject();
                writer.WritePropertyName("some");
                writer.WriteRawValue(JsonConvert.SerializeObject(value));
                writer.WriteEndObject();
            }
            else
            {
                writer.WriteRaw(JsonConvert.SerializeObject(value));
            }
            
            writer.WriteValue(BitConverter.ToString((byte[])value).Replace("-", string.Empty));
        }
    }
}