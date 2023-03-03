using Google.Protobuf.WellKnownTypes;

namespace SpacetimeDB
{
    public class Option<T>
    {
        private readonly bool isSome;
        private readonly T value;
        
        public Option()
        {
            isSome = false;
        }
        
        public Option(T t)
        {
            isSome = true;
            value = t;
        }

        public bool IsNone() => !isSome;

        public bool IsSome() => isSome; 

        public T AsSome() => value;

        public static Option<T> CreateSome(T t) => new Option<T>(t);
        public static Option<T> CreateNone() => new Option<T>();
    }
    
    public class Option
    {
        private readonly object value;

        public Option()
        {
            value = null;
        }
        
        public Option(object value)
        {
            this.value = value;
        }

        public bool IsNone() => value == null;

        public bool IsSome() => value != null;

        public object AsSome() => value;

        public static Option CreateSome(object t) => new Option(t);
        public static Option CreateNone() => new Option();
    }
}