use std::ops::Bound;

use serde::{Deserialize, Serialize};

use crate::schema::SimRow;

use super::TableWorkloadInteraction;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PropertyBound {
    Unbounded,
    Included(SimRow),
    Excluded(SimRow),
}

impl PropertyBound {
    pub fn to_range_bound(&self) -> Bound<spacetimedb_sats::AlgebraicValue> {
        match self {
            Self::Unbounded => Bound::Unbounded,
            Self::Included(key) => Bound::Included(key.to_algebraic_value()),
            Self::Excluded(key) => Bound::Excluded(key.to_algebraic_value()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TableProperty {
    VisibleInConnection {
        conn: usize,
        table: usize,
        row: SimRow,
    },
    MissingInConnection {
        conn: usize,
        table: usize,
        row: SimRow,
    },
    VisibleFresh {
        table: usize,
        row: SimRow,
    },
    MissingFresh {
        table: usize,
        row: SimRow,
    },
    RowCountFresh {
        table: usize,
        expected: u64,
    },
    RangeScanInConnection {
        conn: usize,
        table: usize,
        cols: Vec<u16>,
        lower: PropertyBound,
        upper: PropertyBound,
        expected_rows: Vec<SimRow>,
    },
    RangeScanFresh {
        table: usize,
        cols: Vec<u16>,
        lower: PropertyBound,
        upper: PropertyBound,
        expected_rows: Vec<SimRow>,
    },
    TablesMatchFresh {
        left: usize,
        right: usize,
    },
}

pub(crate) fn property_interaction(property: TableProperty) -> TableWorkloadInteraction {
    TableWorkloadInteraction::Check(property)
}

pub(crate) fn followup_properties_after_commit(
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
