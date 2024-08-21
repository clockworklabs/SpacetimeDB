using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.BSATN;

namespace SpacetimeDB
{
    public interface IDatabaseTable
    {
        void InternalOnValueInserted();
        void InternalOnValueDeleted();
        void OnInsertEvent(ClientApi.Event? dbEvent);
        void OnBeforeDeleteEvent(ClientApi.Event? dbEvent);
        void OnDeleteEvent(ClientApi.Event? dbEvent);
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

        public void OnInsertEvent(ClientApi.Event? dbEvent)
        {
            OnInsert?.Invoke((T)this, (ReducerEvent?)dbEvent?.FunctionCall.CallInfo);
        }

        public void OnBeforeDeleteEvent(ClientApi.Event? dbEvent)
        {
            OnBeforeDelete?.Invoke((T)this, (ReducerEvent?)dbEvent?.FunctionCall.CallInfo);
        }

        public void OnDeleteEvent(ClientApi.Event? dbEvent)
        {
            OnDelete?.Invoke((T)this, (ReducerEvent?)dbEvent?.FunctionCall.CallInfo);
        }
    }

    public interface IDatabaseTableWithPrimaryKey : IDatabaseTable
    {
        void OnUpdateEvent(IDatabaseTableWithPrimaryKey newValue, ClientApi.Event? dbEvent);
        object GetPrimaryKeyValue();
    }

    public abstract class DatabaseTableWithPrimaryKey<T, ReducerEvent> : DatabaseTable<T, ReducerEvent>, IDatabaseTableWithPrimaryKey
        where T : DatabaseTableWithPrimaryKey<T, ReducerEvent>, IStructuralReadWrite, new()
        where ReducerEvent : ReducerEventBase
    {
        public abstract object GetPrimaryKeyValue();

        public delegate void UpdateEventHandler(T oldValue, T newValue, ReducerEvent? dbEvent);
        public static event UpdateEventHandler? OnUpdate;

        public void OnUpdateEvent(IDatabaseTableWithPrimaryKey newValue, ClientApi.Event? dbEvent)
        {
            OnUpdate?.Invoke((T)this, (T)newValue, (ReducerEvent?)dbEvent?.FunctionCall.CallInfo);
        }
    }
}
