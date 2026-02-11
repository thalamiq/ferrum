use std::collections::HashMap;
use std::sync::Arc;

/// Registry for external variables (%resource, %context, etc.)
pub struct VariableRegistry {
    next_id: u16,
    ids_by_name: HashMap<Arc<str>, u16>,
    names_by_id: HashMap<u16, Arc<str>>,
}

impl VariableRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            next_id: 3, // 0=$this, 1=$index, 2=$total
            ids_by_name: HashMap::new(),
            names_by_id: HashMap::new(),
        };

        // Pre-register common context variables (external constants, without leading `%`)
        for name in ["resource", "context", "root", "rootResource", "profile"] {
            let _ = registry.register(name);
        }

        registry
    }

    /// Resolve a variable name to a stable ID, allocating a new one if needed.
    pub fn resolve(&mut self, name: &str) -> u16 {
        if let Some(id) = self.ids_by_name.get(name) {
            return *id;
        }
        self.register(name)
    }

    /// Get the variable name for an ID, if it exists.
    pub fn name_for(&self, var_id: u16) -> Option<Arc<str>> {
        self.names_by_id.get(&var_id).cloned()
    }

    fn register(&mut self, name: &str) -> u16 {
        let arc_name: Arc<str> = Arc::from(name);
        if let Some(id) = self.ids_by_name.get(&arc_name) {
            return *id;
        }
        let id = self.next_id;
        // Prevent accidental overflow; in practice we won't hit u16::MAX
        self.next_id = self.next_id.saturating_add(1);
        self.ids_by_name.insert(arc_name.clone(), id);
        self.names_by_id.insert(id, arc_name);
        id
    }
}

impl Default for VariableRegistry {
    fn default() -> Self {
        Self::new()
    }
}
