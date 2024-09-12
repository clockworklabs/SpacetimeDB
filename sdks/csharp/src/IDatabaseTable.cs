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
        void OnInsertEvent(IEventContext? update);
        void OnBeforeDeleteEvent(IEventContext? update);
        void OnDeleteEvent(IEventContext? update);
    }

    public abstract class DatabaseTable<T, EventContext> : IDatabaseTable
        where T : DatabaseTable<T, EventContext>, IStructuralReadWrite, new()
        where EventContext : IEventContext
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

        public delegate void InsertEventHandler(T insertedValue, EventContext? dbEvent);
        public delegate void DeleteEventHandler(T deletedValue, EventContext? dbEvent);
        public static event InsertEventHandler? OnInsert;
        public static event DeleteEventHandler? OnBeforeDelete;
        public static event DeleteEventHandler? OnDelete;

        public void OnInsertEvent(IEventContext? dbEvent)
        {
            OnInsert?.Invoke((T)this, (EventContext?)dbEvent);
        }

        public void OnBeforeDeleteEvent(IEventContext? dbEvent)
        {
            OnBeforeDelete?.Invoke((T)this, (EventContext?)dbEvent);
        }

        public void OnDeleteEvent(IEventContext? dbEvent)
        {
            OnDelete?.Invoke((T)this, (EventContext?)dbEvent);
        }
    }

    public interface IDatabaseTableWithPrimaryKey : IDatabaseTable
    {
        void OnUpdateEvent(IDatabaseTableWithPrimaryKey newValue, IEventContext? update);
        object GetPrimaryKeyValue();
    }

    public abstract class DatabaseTableWithPrimaryKey<T, EventContext> : DatabaseTable<T, EventContext>, IDatabaseTableWithPrimaryKey
        where T : DatabaseTableWithPrimaryKey<T, EventContext>, IStructuralReadWrite, new()
        where EventContext : IEventContext
    {
        public abstract object GetPrimaryKeyValue();

        public delegate void UpdateEventHandler(T oldValue, T newValue, EventContext? update);
        public static event UpdateEventHandler? OnUpdate;

        public void OnUpdateEvent(IDatabaseTableWithPrimaryKey newValue, IEventContext? dbEvent)
        {
            OnUpdate?.Invoke((T)this, (T)newValue, (EventContext?)dbEvent);
        }
    }
}
