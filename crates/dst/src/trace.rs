#[derive(Clone, Debug)]
pub struct Trace<E> {
    events: Vec<E>,
}

impl<E> Trace<E> {
    pub fn push(&mut self, event: E) {
        self.events.push(event);
    }

    pub fn as_slice(&self) -> &[E] {
        &self.events
    }

    pub fn into_events(self) -> Vec<E> {
        self.events
    }
}

impl<E> Default for Trace<E> {
    fn default() -> Self {
        Self { events: Vec::new() }
    }
}
