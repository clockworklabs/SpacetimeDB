import { schema, table, t } from 'spacetimedb/server';

const organization = table(
  { name: 'organization', public: true },
  { id: t.u64().primaryKey().autoInc(), name: t.string() }
);

const department = table(
  {
    name: 'department',
    public: true,
    indexes: [{ accessor: 'byOrganization', algorithm: 'btree', columns: ['organizationId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    organizationId: t.u64(),
    name: t.string(),
  }
);

const employee = table(
  {
    name: 'employee',
    public: true,
    indexes: [{ accessor: 'byDepartment', algorithm: 'btree', columns: ['departmentId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    departmentId: t.u64(),
    name: t.string(),
  }
);

export default schema({ organization, department, employee });
