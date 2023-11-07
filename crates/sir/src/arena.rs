use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct Arena<Key, Value> {
    env: Vec<Value>,
    pub(crate) names: HashMap<Key, usize>,
}

impl<Key, Value> Arena<Key, Value>
where
    Key: PartialEq + Eq + Hash,
{
    pub fn new() -> Self {
        Self {
            env: Vec::new(),
            names: HashMap::new(),
        }
    }

    pub fn next_id(&self) -> usize {
        self.env.len()
    }

    pub fn add<N: Into<Key>>(&mut self, name: N, value: Value) -> usize {
        let idx = self.env.len();
        self.env.push(value);
        self.names.insert(name.into(), idx);
        idx
    }

    pub fn update<N: Into<Key>>(&mut self, name: N, value: Value) -> bool {
        if let Some(id) = self.get_key_id(&(name.into())) {
            self.env[id] = value;
            true
        } else {
            false
        }
    }

    pub fn get(&self, key: usize) -> Option<&Value> {
        self.env.get(key)
    }

    pub fn get_key_id(&self, key: &Key) -> Option<usize> {
        self.names.get(key).cloned()
    }

    pub fn get_by_key(&self, name: &Key) -> Option<&Value> {
        if let Some(k) = self.names.get(name) {
            self.get(*k)
        } else {
            None
        }
    }
}
