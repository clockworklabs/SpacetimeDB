#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StampedEvent<E> {
    pub step_id: u64,
    pub logical_time: Option<u64>,
    pub actor_id: Option<u64>,
    pub resource_id: Option<u64>,
    pub event: E,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trace<E> {
    events: Vec<StampedEvent<E>>,
    next_step_id: u64,
}

impl<E> Trace<E> {
    pub fn from_events(events: Vec<E>) -> Self {
        let mut trace = Self::default();
        for event in events {
            trace.push(event);
        }
        trace
    }

    pub fn push(&mut self, event: E) {
        self.push_stamped(StampedEvent {
            step_id: self.next_step_id,
            logical_time: None,
            actor_id: None,
            resource_id: None,
            event,
        });
    }

    pub fn push_with_meta(
        &mut self,
        event: E,
        logical_time: Option<u64>,
        actor_id: Option<u64>,
        resource_id: Option<u64>,
    ) {
        self.push_stamped(StampedEvent {
            step_id: self.next_step_id,
            logical_time,
            actor_id,
            resource_id,
            event,
        });
    }

    pub fn as_slice(&self) -> &[StampedEvent<E>] {
        &self.events
    }

    pub fn into_events(self) -> Vec<StampedEvent<E>> {
        self.events
    }

    fn push_stamped(&mut self, stamped: StampedEvent<E>) {
        self.next_step_id = stamped.step_id + 1;
        self.events.push(stamped);
    }
}

impl<E> Default for Trace<E> {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            next_step_id: 0,
        }
    }
}
