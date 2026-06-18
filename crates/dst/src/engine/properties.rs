use super::workload::{Interaction, Observation};
use crate::traits::Properties;

pub struct EngineProperties;

impl Properties<Interaction, Observation> for EngineProperties {
    fn observe(&mut self, interaction: &Interaction, observation: &Observation) -> Result<(), anyhow::Error> {
        Ok(())
        //  match interaction {
        //      Interaction::Insert { table, row } => todo!(),
        //      Interaction::Delete { table, row } => todo!(),
        //      Interaction::Count { table } => todo!(),
        //  }
    }
}
