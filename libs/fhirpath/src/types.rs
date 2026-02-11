//! Type system for FHIRPath
//!
//! FHIRPath is a collection-oriented language: every expression yields a collection,
//! and type checking is primarily about the element type set and cardinality.
//!
//! This module provides:
//! - `TypeId` + `TypeRegistry`: System primitive types (and aliases)
//! - `NamedType` / `TypeSet`: fully-qualified types (System vs FHIR) + unions (choice types)
//! - `Cardinality` / `ExprType`: element type set + collection cardinality for HIR nodes

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

/// System primitive types used by FHIRPath typing rules.
///
/// Note: `Unknown` means "not statically known" (polymorphic/context-dependent),
/// not the same as an empty collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum TypeId {
    Unknown = 0, // Placeholder type before type pass runs
    Boolean = 1,
    Integer = 2,
    String = 3,
    Decimal = 4,
    Date = 5,
    DateTime = 6,
    Time = 7,
    Quantity = 8,
}

/// Fully-qualified type namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeNamespace {
    System,
    Fhir,
}

/// Fully-qualified type name used by the type checker.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamedType {
    pub namespace: TypeNamespace,
    pub name: Arc<str>,
}

impl Ord for NamedType {
    fn cmp(&self, other: &Self) -> Ordering {
        self.namespace
            .cmp(&other.namespace)
            .then_with(|| self.name.cmp(&other.name))
    }
}

impl PartialOrd for NamedType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A set of possible element types (used for choice types and unions).
///
/// Invariants:
/// - Sorted, de-duplicated
/// - Empty set means "unknown / any" (not "empty collection")
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSet(Arc<[NamedType]>);

impl TypeSet {
    pub fn unknown() -> Self {
        Self(Arc::from([]))
    }

    pub fn singleton(ty: NamedType) -> Self {
        Self(Arc::from([ty]))
    }

    pub fn is_unknown(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &NamedType> {
        self.0.iter()
    }

    pub fn union(&self, other: &TypeSet) -> TypeSet {
        if self.is_unknown() || other.is_unknown() {
            return TypeSet::unknown();
        }

        let mut merged: Vec<NamedType> = Vec::with_capacity(self.0.len() + other.0.len());
        merged.extend(self.0.iter().cloned());
        merged.extend(other.0.iter().cloned());

        merged.sort();
        merged.dedup();
        TypeSet(Arc::from(merged))
    }

    pub fn from_many(mut types: Vec<NamedType>) -> TypeSet {
        if types.is_empty() {
            return TypeSet::unknown();
        }
        types.sort();
        types.dedup();
        TypeSet(Arc::from(types))
    }
}

/// Cardinality of a FHIRPath expression result collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cardinality {
    pub min: u32,
    pub max: Option<u32>, // None = unbounded
}

impl Cardinality {
    pub const EMPTY: Cardinality = Cardinality {
        min: 0,
        max: Some(0),
    };

    pub const ZERO_TO_ONE: Cardinality = Cardinality {
        min: 0,
        max: Some(1),
    };

    pub const ZERO_TO_MANY: Cardinality = Cardinality { min: 0, max: None };

    pub const ONE_TO_ONE: Cardinality = Cardinality {
        min: 1,
        max: Some(1),
    };

    pub fn at_most_one(self) -> Cardinality {
        Cardinality {
            min: 0,
            max: Some(1),
        }
    }

    pub fn multiply(self, other: Cardinality) -> Cardinality {
        let min = self.min.saturating_mul(other.min);
        let max = match (self.max, other.max) {
            (Some(a), Some(b)) => Some(a.saturating_mul(b)),
            _ => None,
        };
        Cardinality { min, max }
    }

    pub fn add_upper_bounds(self, other: Cardinality) -> Cardinality {
        let max = match (self.max, other.max) {
            (Some(a), Some(b)) => Some(a.saturating_add(b)),
            _ => None,
        };
        Cardinality { min: 0, max }
    }
}

/// Static type annotation for a HIR node: element type set + cardinality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprType {
    pub types: TypeSet,
    pub cardinality: Cardinality,
}

impl ExprType {
    pub fn unknown() -> Self {
        Self {
            types: TypeSet::unknown(),
            cardinality: Cardinality::ZERO_TO_MANY,
        }
    }

    pub fn empty() -> Self {
        Self {
            types: TypeSet::unknown(),
            cardinality: Cardinality::EMPTY,
        }
    }

    pub fn with_cardinality(mut self, cardinality: Cardinality) -> Self {
        self.cardinality = cardinality;
        self
    }
}

/// Type registry for System types.
///
/// FHIR types are resolved via `FhirContext` in the type checker and represented as
/// `NamedType { namespace: Fhir, name: ... }`.
pub struct TypeRegistry {
    /// Map from TypeId to NamedType (indexed by TypeId value)
    pub(crate) types_by_id: Vec<Option<NamedType>>,
    /// Map from type name / alias to TypeId
    pub(crate) types_by_name: HashMap<Arc<str>, TypeId>,
}

impl TypeRegistry {
    /// Create a new type registry with System types initialized
    pub fn new() -> Self {
        let mut registry = Self {
            types_by_id: Vec::new(),
            types_by_name: HashMap::new(),
        };

        // Initialize system types
        registry.init_system_types();

        registry
    }

    pub fn system_named(&self, id: TypeId) -> NamedType {
        let idx = id as usize;
        self.types_by_id
            .get(idx)
            .and_then(|v| v.clone())
            .unwrap_or_else(|| NamedType {
                namespace: TypeNamespace::System,
                name: Arc::from("Unknown"),
            })
    }

    pub fn system_set(&self, id: TypeId) -> TypeSet {
        if id == TypeId::Unknown {
            return TypeSet::unknown();
        }
        TypeSet::singleton(self.system_named(id))
    }

    pub fn fhir_named(&self, name: &str) -> NamedType {
        NamedType {
            namespace: TypeNamespace::Fhir,
            name: Arc::from(name),
        }
    }

    pub fn boolean(&self) -> ExprType {
        ExprType {
            types: self.system_set(TypeId::Boolean),
            cardinality: Cardinality::ZERO_TO_ONE,
        }
    }

    pub fn integer(&self) -> ExprType {
        ExprType {
            types: self.system_set(TypeId::Integer),
            cardinality: Cardinality::ZERO_TO_ONE,
        }
    }

    pub fn string(&self) -> ExprType {
        ExprType {
            types: self.system_set(TypeId::String),
            cardinality: Cardinality::ZERO_TO_ONE,
        }
    }

    pub fn decimal(&self) -> ExprType {
        ExprType {
            types: self.system_set(TypeId::Decimal),
            cardinality: Cardinality::ZERO_TO_ONE,
        }
    }

    pub fn date(&self) -> ExprType {
        ExprType {
            types: self.system_set(TypeId::Date),
            cardinality: Cardinality::ZERO_TO_ONE,
        }
    }

    pub fn datetime(&self) -> ExprType {
        ExprType {
            types: self.system_set(TypeId::DateTime),
            cardinality: Cardinality::ZERO_TO_ONE,
        }
    }

    pub fn time(&self) -> ExprType {
        ExprType {
            types: self.system_set(TypeId::Time),
            cardinality: Cardinality::ZERO_TO_ONE,
        }
    }

    pub fn quantity(&self) -> ExprType {
        ExprType {
            types: self.system_set(TypeId::Quantity),
            cardinality: Cardinality::ZERO_TO_ONE,
        }
    }

    /// Get type name from TypeId
    pub fn get_type_name(&self, type_id: TypeId) -> Option<Arc<str>> {
        let idx = type_id as usize;
        self.types_by_id
            .get(idx)
            .and_then(|t| t.clone())
            .map(|t| t.name)
    }

    /// Get TypeId from type name (System types only)
    pub fn get_type_id_by_name(&self, name: &str) -> Option<TypeId> {
        self.types_by_name.get(name).copied()
    }

    pub fn is_system_type_name(&self, name: &str) -> bool {
        self.get_type_id_by_name(name).is_some()
    }

    /// Initialize System primitive types
    fn init_system_types(&mut self) {
        let system_types = vec![
            (TypeId::Unknown, "Unknown"),
            (TypeId::Boolean, "Boolean"),
            (TypeId::Integer, "Integer"),
            (TypeId::String, "String"),
            (TypeId::Decimal, "Decimal"),
            (TypeId::Date, "Date"),
            (TypeId::DateTime, "DateTime"),
            (TypeId::Time, "Time"),
            (TypeId::Quantity, "Quantity"),
        ];

        for (id, name) in system_types {
            let named = NamedType {
                namespace: TypeNamespace::System,
                name: Arc::from(name),
            };
            // Ensure vector is large enough
            let id_value = id as usize;
            if self.types_by_id.len() <= id_value {
                self.types_by_id.resize(id_value + 1, None);
            }
            self.types_by_id[id_value] = Some(named.clone());
            self.types_by_name.insert(Arc::from(name), id);
        }

        // Add System.* aliases for primitives for easier lookup of type specifiers
        let system_aliases = [
            (TypeId::Unknown, "System.Unknown"),
            (TypeId::Boolean, "System.Boolean"),
            (TypeId::Integer, "System.Integer"),
            (TypeId::String, "System.String"),
            (TypeId::Decimal, "System.Decimal"),
            (TypeId::Date, "System.Date"),
            (TypeId::DateTime, "System.DateTime"),
            (TypeId::Time, "System.Time"),
            (TypeId::Quantity, "System.Quantity"),
        ];

        for (id, alias) in system_aliases {
            self.types_by_name.entry(Arc::from(alias)).or_insert(id);
        }

        // FHIR primitive aliases (lowercase codes) commonly found in StructureDefinitions.
        // These map to System primitives for FHIRPath typing purposes.
        let fhir_primitive_aliases = [
            (TypeId::String, "string"),
            (TypeId::String, "uri"),
            (TypeId::String, "canonical"),
            (TypeId::String, "code"),
            (TypeId::String, "id"),
            (TypeId::String, "oid"),
            (TypeId::String, "uuid"),
            (TypeId::String, "markdown"),
            (TypeId::Boolean, "boolean"),
            (TypeId::Integer, "integer"),
            (TypeId::Integer, "positiveInt"),
            (TypeId::Integer, "unsignedInt"),
            (TypeId::Decimal, "decimal"),
            (TypeId::Date, "date"),
            (TypeId::DateTime, "dateTime"),
            (TypeId::Time, "time"),
        ];

        for (id, alias) in fhir_primitive_aliases {
            self.types_by_name.entry(Arc::from(alias)).or_insert(id);
        }
    }

    /// Check if type1 is a subtype of type2 (for System types only)
    ///
    /// Note: FHIR type subtype checking is handled by FhirContext at runtime.
    pub fn is_subtype_of(&self, type1: TypeId, type2: TypeId) -> bool {
        // For System types, only exact matches are subtypes
        type1 == type2
    }

    /// Infer a System `TypeId` from a runtime `Value` (best-effort).
    pub fn infer_system_type_from_value(
        &self,
        value: &crate::value::Value,
        _path: Option<&str>,
    ) -> Option<TypeId> {
        use crate::value::ValueData;

        match value.data() {
            // Primitive types (direct mapping)
            ValueData::Boolean(_) => Some(TypeId::Boolean),
            ValueData::Integer(_) => Some(TypeId::Integer),
            ValueData::Decimal(_) => Some(TypeId::Decimal),
            ValueData::String(_) => Some(TypeId::String),
            ValueData::Date { .. } => Some(TypeId::Date),
            ValueData::DateTime {
                value: _,
                precision: _,
                timezone_offset: _,
            } => Some(TypeId::DateTime),
            ValueData::Time {
                value: _,
                precision: _,
            } => Some(TypeId::Time),
            ValueData::Quantity { .. } => Some(TypeId::Quantity),

            // Complex types (objects) - single path with 2 priorities
            ValueData::Object(obj_map) => {
                // PRIORITY 1 (REMOVED): Path lookup via PATH_TYPE_MAP
                // This was removed to save ~200MB of generated code
                // Type inference now relies on resourceType and structural signatures

                // PRIORITY 1: resourceType field (for FHIR resources)
                if let Some(resource_type_col) = obj_map.get("resourceType") {
                    if let Some(resource_type_val) = resource_type_col.iter().next() {
                        if let ValueData::String(rt) = resource_type_val.data() {
                            // Resource type is a FHIR type; we can't map that to a System TypeId.
                            let _ = rt;
                        }
                    }
                }

                // No match found - type inference for complex types requires FhirContext
                None
            }

            ValueData::LazyJson { .. } => {
                // Materialize lazy JSON and treat as object
                let materialized = value.materialize();
                self.infer_system_type_from_value(&materialized, _path)
            }

            ValueData::Empty => None,
        }
    }

    pub fn expr_from_system_type(&self, id: TypeId, cardinality: Cardinality) -> ExprType {
        ExprType {
            types: self.system_set(id),
            cardinality,
        }
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn test_type_registry_creation() {
        let registry = TypeRegistry::new();

        // Test System types are initialized
        assert!(registry.get_type_name(TypeId::Boolean).is_some());
        assert!(registry.get_type_name(TypeId::String).is_some());
        assert!(registry.get_type_name(TypeId::Integer).is_some());
    }

    #[test]
    fn test_get_type_by_name() {
        let registry = TypeRegistry::new();

        assert!(registry.get_type_id_by_name("Boolean").is_some());
        assert!(registry.get_type_id_by_name("String").is_some());
        assert!(registry.get_type_id_by_name("Nonexistent").is_none());
    }

    #[test]
    fn test_infer_type_from_value() {
        let registry = TypeRegistry::new();

        assert_eq!(
            registry.infer_system_type_from_value(&Value::boolean(true), None),
            Some(TypeId::Boolean)
        );
        assert_eq!(
            registry.infer_system_type_from_value(&Value::integer(42), None),
            Some(TypeId::Integer)
        );
        assert_eq!(
            registry.infer_system_type_from_value(&Value::string("test"), None),
            Some(TypeId::String)
        );
    }

    #[test]
    fn test_subtype_checking() {
        let registry = TypeRegistry::new();

        // Reflexive subtype checks should succeed
        assert!(registry.is_subtype_of(TypeId::String, TypeId::String));

        // Unrelated types should remain false
        assert!(!registry.is_subtype_of(TypeId::String, TypeId::Boolean));
    }

    #[test]
    fn test_type_set_union() {
        let reg = TypeRegistry::new();
        let a = TypeSet::singleton(reg.system_named(TypeId::String));
        let b = TypeSet::singleton(reg.fhir_named("Patient"));
        let u = a.union(&b);
        assert!(!u.is_unknown());
        assert_eq!(u.iter().count(), 2);
    }
}
