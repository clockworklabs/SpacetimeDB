import { schema, table, t } from 'spacetimedb/server';

const userProfile = table(
  { name: 'user_profile', public: true },
  {
    identity: t.identity().primaryKey(),
    displayName: t.string(),
    createdAt: t.timestamp(),
  }
);

const connectionPresence = table(
  {
    name: 'connection_presence',
    public: true,
    indexes: [{ accessor: 'byUser', algorithm: 'btree', columns: ['userIdentity'] }],
  },
  {
    connectionId: t.connectionId().primaryKey(),
    userIdentity: t.identity(),
    lastHeartbeat: t.timestamp(),
    online: t.bool(),
  }
);

export default schema({ userProfile, connectionPresence });
