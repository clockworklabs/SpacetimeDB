using System;
using System.Collections.Generic;
using Newtonsoft.Json;

namespace SpacetimeDB
{
    public class OptionConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType)
        {
            return true;
        }
        
        public override object ReadJson(JsonReader reader, Type objectType, object existingValue, JsonSerializer serializer)
        {
            
        }

        struct SomeValue
        {
            public object some;
        }

        public override void WriteJson(JsonWriter writer, object value, JsonSerializer serializer)
        {
            if (value != null)
            {
                writer.WriteValue(new SomeValue
                {
                    some = value,
                });
            }
            else
            {
                writer.WriteValue("{ \"none\": [] }");
            }
        }
    }
}
