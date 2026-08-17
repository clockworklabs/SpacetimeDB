use spacetimedb::table;

#[table(accessor = organization, public)]
pub struct Organization {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
}

#[table(
    accessor = department,
    public,
    index(accessor = by_organization, btree(columns = [organization_id]))
)]
pub struct Department {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub organization_id: u64,
    pub name: String,
}

#[table(
    accessor = employee,
    public,
    index(accessor = by_department, btree(columns = [department_id]))
)]
pub struct Employee {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub department_id: u64,
    pub name: String,
}
