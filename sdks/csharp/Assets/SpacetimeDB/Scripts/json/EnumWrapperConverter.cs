using System;
using System.Collections;
using System.Collections.Generic;
using Namespace;
using Newtonsoft.Json;
using UnityEngine;

namespace SpacetimeDB 
{
    public class EnumWrapperConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType) => objectType == typeof(EnumWrapper<>);

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
            writer.WritePropertyName(value.ToString());
            writer.WriteRaw("{}");
            writer.WriteEndObject();

        }
    }
}
