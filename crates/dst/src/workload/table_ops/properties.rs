use serde::{Deserialize, Serialize};

use crate::schema::SimRow;

use super::TableWorkloadInteraction;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TableProperty {
    VisibleInConnection { conn: usize, table: usize, row: SimRow },
    MissingInConnection { conn: usize, table: usize, row: SimRow },
    VisibleFresh { table: usize, row: SimRow },
    MissingFresh { table: usize, row: SimRow },
    RowCountFresh { table: usize, expected: u64 },
    TablesMatchFresh { left: usize, right: usize },
}

pub fn property_interaction(property: TableProperty) -> TableWorkloadInteraction {
    TableWorkloadInteraction::Check(property)
}

pub fn followup_properties_after_commit(
    scenario_commit_properties: Vec<TableWorkloadInteraction>,
    inserts: Vec<(usize, SimRow)>,
    deletes: Vec<(usize, SimRow)>,
) -> Vec<TableWorkloadInteraction> {
    let mut followups = Vec::new();
    for (table, row) in inserts {
        followups.push(property_interaction(TableProperty::VisibleFresh { table, row }));
    }
    for (table, row) in deletes {
        followups.push(property_interaction(TableProperty::MissingFresh { table, row }));
    }
    followups.extend(scenario_commit_properties);
    followups
}
