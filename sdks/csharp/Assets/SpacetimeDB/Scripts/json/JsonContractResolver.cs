using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using System;
using Newtonsoft.Json;
using Newtonsoft.Json.Serialization;
using UnityEngine;

namespace SpacetimeDB
{
    public class SomeConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType) => true;

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
    
    public class JsonContractResolver : DefaultContractResolver
    {
        protected override JsonProperty CreateProperty(MemberInfo member, MemberSerialization memberSerialization)
        {
            var property = base.CreateProperty(member, memberSerialization);

            if (member.GetCustomAttribute<SomeAttribute>() != null)
            {
                property.Converter = new SomeConverter();
            } else if (member.GetCustomAttribute<SpacetimeDB.EnumAttribute>() != null)
            {
                property.Converter = new EnumConverter();
            }

            return property;
        }
    }
}
