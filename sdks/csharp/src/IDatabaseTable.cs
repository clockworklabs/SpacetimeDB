using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;

namespace SpacetimeDB
{
    public interface IDatabaseTable
    {
        void InternalOnValueInserted();
        void InternalOnValueDeleted();
        void OnInsertEvent(ReducerEventBase? update);
        void OnBeforeDeleteEvent(ReducerEventBase? update);
        void OnDeleteEvent(ReducerEventBase? update);
    }

    public abstract class DatabaseTable<T, ReducerEvent> : IDatabaseTable
        where T : DatabaseTable<T, ReducerEvent>, IStructuralReadWrite, new()
        where ReducerEvent : ReducerEventBase
    {
        public virtual void InternalOnValueInserted() { }

        public virtual void InternalOnValueDeleted() { }

        public static IEnumerable<T> Iter()
        {
            return ClientCache.TableCache<T>.Entries.Values;
        }

        public static IEnumerable<T> Query(Func<T, bool> filter)
        {
            return Iter().Where(filter);
        }

        public static int Count()
        {
            return ClientCache.TableCache<T>.Entries.Count;
        }

        public delegate void InsertEventHandler(T insertedValue, ReducerEvent? dbEvent);
        public delegate void DeleteEventHandler(T deletedValue, ReducerEvent? dbEvent);
        public static event InsertEventHandler? OnInsert;
        public static event DeleteEventHandler? OnBeforeDelete;
        public static event DeleteEventHandler? OnDelete;

        public void OnInsertEvent(ReducerEventBase? dbEvent)
        {
            OnInsert?.Invoke((T)this, (ReducerEvent?)dbEvent);
        }

        public void OnBeforeDeleteEvent(ReducerEventBase? dbEvent)
        {
            OnBeforeDelete?.Invoke((T)this, (ReducerEvent?)dbEvent);
        }

        public void OnDeleteEvent(ReducerEventBase? dbEvent)
        {
            OnDelete?.Invoke((T)this, (ReducerEvent?)dbEvent);
        }
    }

    public interface IDatabaseTableWithPrimaryKey : IDatabaseTable
    {
        void OnUpdateEvent(IDatabaseTableWithPrimaryKey newValue, ReducerEventBase? update);
        object GetPrimaryKeyValue();
    }

    public abstract class DatabaseTableWithPrimaryKey<T, ReducerEvent> : DatabaseTable<T, ReducerEvent>, IDatabaseTableWithPrimaryKey
        where T : DatabaseTableWithPrimaryKey<T, ReducerEvent>, IStructuralReadWrite, new()
        where ReducerEvent : ReducerEventBase
    {
        public abstract object GetPrimaryKeyValue();

        public delegate void UpdateEventHandler(T oldValue, T newValue, ReducerEvent? update);
        public static event UpdateEventHandler? OnUpdate;

        public void OnUpdateEvent(IDatabaseTableWithPrimaryKey newValue, ReducerEventBase? dbEvent)
        {
            OnUpdate?.Invoke((T)this, (T)newValue, (ReducerEvent?)dbEvent);
        }
    }
}
