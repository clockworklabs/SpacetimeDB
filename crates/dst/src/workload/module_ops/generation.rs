use crate::{
    core::NextInteractionSource,
    schema::generate_value_for_type,
    seed::{DstRng, DstSeed},
    workload::strategy::{Index, Strategy, Weighted},
};

use super::{HostScenarioId, ModuleInteraction, ModuleReducerSpec};

const MAX_REGEN_ATTEMPTS: usize = 16;

#[derive(Clone, Copy, Debug)]
enum ActionKind {
    Reducer,
    Wait,
    Reopen,
}

/// Deterministic source for standalone-host interactions.
pub(crate) struct ModuleWorkloadSource {
    scenario: HostScenarioId,
    reducers: Vec<ModuleReducerSpec>,
    rng: DstRng,
    target_interactions: usize,
    emitted: usize,
}

impl ModuleWorkloadSource {
    pub fn new(
        seed: DstSeed,
        scenario: HostScenarioId,
        reducers: Vec<ModuleReducerSpec>,
        target_interactions: usize,
    ) -> Self {
        Self {
            scenario,
            reducers,
            rng: seed.fork(300).rng(),
            target_interactions,
            emitted: 0,
        }
    }

    pub fn request_finish(&mut self) {
        self.target_interactions = self.emitted;
    }

    fn choose_action(&mut self) -> ActionKind {
        match self.scenario {
            HostScenarioId::HostSmoke => Weighted::new(vec![
                (85, ActionKind::Reducer),
                (10, ActionKind::Wait),
                (5, ActionKind::Reopen),
            ])
            .sample(&mut self.rng),
        }
    }

    fn generate_reducer_interaction(&mut self) -> Option<ModuleInteraction> {
        if self.reducers.is_empty() {
            return None;
        }
        let idx = Index::new(self.reducers.len()).sample(&mut self.rng);
        let spec = &self.reducers[idx];
        let mut args = Vec::with_capacity(spec.params.len());
        for (arg_index, ty) in spec.params.iter().enumerate() {
            if !supports_generation(ty) {
                return None;
            }
            args.push(generate_value_for_type(&mut self.rng, ty, arg_index));
        }
        Some(ModuleInteraction::CallReducer {
            reducer: spec.name.clone(),
            args,
        })
    }

    fn generate_next(&mut self) -> ModuleInteraction {
        for _ in 0..MAX_REGEN_ATTEMPTS {
            let next = match self.choose_action() {
                ActionKind::Reducer => self.generate_reducer_interaction(),
                ActionKind::Wait => Some(ModuleInteraction::WaitScheduled { millis: 1_200 }),
                ActionKind::Reopen => Some(ModuleInteraction::CloseReopen),
            };
            if let Some(next) = next {
                return next;
            }
        }
        ModuleInteraction::NoOp
    }
}

fn supports_generation(ty: &spacetimedb_sats::AlgebraicType) -> bool {
    use spacetimedb_sats::AlgebraicType;
    matches!(
        ty,
        AlgebraicType::Bool
            | AlgebraicType::I8
            | AlgebraicType::U8
            | AlgebraicType::I16
            | AlgebraicType::U16
            | AlgebraicType::I32
            | AlgebraicType::U32
            | AlgebraicType::I64
            | AlgebraicType::U64
            | AlgebraicType::I128
            | AlgebraicType::U128
            | AlgebraicType::String
    )
}

impl NextInteractionSource for ModuleWorkloadSource {
    type Interaction = ModuleInteraction;

    fn next_interaction(&mut self) -> Option<Self::Interaction> {
        if self.emitted >= self.target_interactions {
            return None;
        }
        self.emitted += 1;
        Some(self.generate_next())
    }

    fn request_finish(&mut self) {
        Self::request_finish(self);
    }
}
