//! Function registry for FHIRPath functions
//!
//! Maps function names to FunctionId and provides metadata about function signatures.
//!
//! Uses a compile-time perfect hash map (phf) for O(1) function name lookups with zero runtime allocation.

use crate::hir::FunctionId;
use crate::types::TypeId;
use phf::phf_map;

/// Function metadata
#[derive(Debug, Clone, Copy)]
pub struct FunctionMetadata {
    pub id: FunctionId,
    pub name: &'static str,
    pub min_args: usize,
    pub max_args: Option<usize>, // None = unbounded
    pub return_type: TypeId,     // Return type (Unknown if polymorphic/context-dependent)
}

/// Static compile-time function registry using perfect hash map
/// This provides O(1) lookups with zero runtime allocation
static FUNCTIONS_BY_NAME: phf::Map<&'static str, FunctionMetadata> = phf_map! {
    // Boolean logic functions
    "not" => FunctionMetadata { id: 0, name: "not", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "as" => FunctionMetadata { id: 1, name: "as", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },

    // Existence functions
    "empty" => FunctionMetadata { id: 10, name: "empty", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "exists" => FunctionMetadata { id: 11, name: "exists", min_args: 0, max_args: Some(1), return_type: TypeId::Boolean },
    "all" => FunctionMetadata { id: 12, name: "all", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "allTrue" => FunctionMetadata { id: 13, name: "allTrue", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "anyTrue" => FunctionMetadata { id: 14, name: "anyTrue", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "allFalse" => FunctionMetadata { id: 15, name: "allFalse", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "anyFalse" => FunctionMetadata { id: 16, name: "anyFalse", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "subsetOf" => FunctionMetadata { id: 17, name: "subsetOf", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "supersetOf" => FunctionMetadata { id: 18, name: "supersetOf", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "count" => FunctionMetadata { id: 19, name: "count", min_args: 0, max_args: Some(0), return_type: TypeId::Integer },
    "distinct" => FunctionMetadata { id: 20, name: "distinct", min_args: 0, max_args: Some(0), return_type: TypeId::Unknown },
    "isDistinct" => FunctionMetadata { id: 21, name: "isDistinct", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },

    // Filtering functions
    "where" => FunctionMetadata { id: 30, name: "where", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "select" => FunctionMetadata { id: 31, name: "select", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "repeat" => FunctionMetadata { id: 32, name: "repeat", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "ofType" => FunctionMetadata { id: 33, name: "ofType", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "extension" => FunctionMetadata { id: 34, name: "extension", min_args: 1, max_args: Some(2), return_type: TypeId::Unknown },

    // Subsetting functions
    "single" => FunctionMetadata { id: 40, name: "single", min_args: 0, max_args: Some(0), return_type: TypeId::Unknown },
    "first" => FunctionMetadata { id: 41, name: "first", min_args: 0, max_args: Some(0), return_type: TypeId::Unknown },
    "last" => FunctionMetadata { id: 42, name: "last", min_args: 0, max_args: Some(0), return_type: TypeId::Unknown },
    "tail" => FunctionMetadata { id: 43, name: "tail", min_args: 0, max_args: Some(0), return_type: TypeId::Unknown },
    "skip" => FunctionMetadata { id: 44, name: "skip", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "take" => FunctionMetadata { id: 45, name: "take", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "intersect" => FunctionMetadata { id: 46, name: "intersect", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "exclude" => FunctionMetadata { id: 47, name: "exclude", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },

    // Combining functions
    "union" => FunctionMetadata { id: 50, name: "union", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "combine" => FunctionMetadata { id: 51, name: "combine", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },

    // String functions
    "toString" => FunctionMetadata { id: 100, name: "toString", min_args: 0, max_args: Some(0), return_type: TypeId::String },
    "indexOf" => FunctionMetadata { id: 101, name: "indexOf", min_args: 1, max_args: Some(1), return_type: TypeId::Integer },
    "lastIndexOf" => FunctionMetadata { id: 102, name: "lastIndexOf", min_args: 1, max_args: Some(1), return_type: TypeId::Integer },
    "substring" => FunctionMetadata { id: 103, name: "substring", min_args: 1, max_args: Some(2), return_type: TypeId::String },
    "startsWith" => FunctionMetadata { id: 104, name: "startsWith", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "endsWith" => FunctionMetadata { id: 105, name: "endsWith", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "contains" => FunctionMetadata { id: 106, name: "contains", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "upper" => FunctionMetadata { id: 107, name: "upper", min_args: 0, max_args: Some(0), return_type: TypeId::String },
    "lower" => FunctionMetadata { id: 108, name: "lower", min_args: 0, max_args: Some(0), return_type: TypeId::String },
    "replace" => FunctionMetadata { id: 109, name: "replace", min_args: 2, max_args: Some(2), return_type: TypeId::String },
    "matches" => FunctionMetadata { id: 110, name: "matches", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "matchesFull" => FunctionMetadata { id: 111, name: "matchesFull", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "replaceMatches" => FunctionMetadata { id: 112, name: "replaceMatches", min_args: 2, max_args: Some(2), return_type: TypeId::String },
    "length" => FunctionMetadata { id: 113, name: "length", min_args: 0, max_args: Some(0), return_type: TypeId::Integer },
    "toChars" => FunctionMetadata { id: 114, name: "toChars", min_args: 0, max_args: Some(0), return_type: TypeId::String },
    "trim" => FunctionMetadata { id: 115, name: "trim", min_args: 0, max_args: Some(0), return_type: TypeId::String },
    "encode" => FunctionMetadata { id: 116, name: "encode", min_args: 1, max_args: Some(1), return_type: TypeId::String },
    "decode" => FunctionMetadata { id: 117, name: "decode", min_args: 1, max_args: Some(1), return_type: TypeId::String },
    "escape" => FunctionMetadata { id: 118, name: "escape", min_args: 1, max_args: Some(1), return_type: TypeId::String },
    "unescape" => FunctionMetadata { id: 119, name: "unescape", min_args: 1, max_args: Some(1), return_type: TypeId::String },
    "split" => FunctionMetadata { id: 120, name: "split", min_args: 1, max_args: Some(1), return_type: TypeId::String },
    "join" => FunctionMetadata { id: 121, name: "join", min_args: 1, max_args: Some(1), return_type: TypeId::String },

    // Math functions
    "abs" => FunctionMetadata { id: 200, name: "abs", min_args: 0, max_args: Some(0), return_type: TypeId::Unknown },
    "ceiling" => FunctionMetadata { id: 201, name: "ceiling", min_args: 0, max_args: Some(0), return_type: TypeId::Integer },
    "exp" => FunctionMetadata { id: 202, name: "exp", min_args: 0, max_args: Some(0), return_type: TypeId::Decimal },
    "floor" => FunctionMetadata { id: 203, name: "floor", min_args: 0, max_args: Some(0), return_type: TypeId::Integer },
    "ln" => FunctionMetadata { id: 204, name: "ln", min_args: 0, max_args: Some(0), return_type: TypeId::Decimal },
    "log" => FunctionMetadata { id: 205, name: "log", min_args: 1, max_args: Some(1), return_type: TypeId::Decimal },
    "power" => FunctionMetadata { id: 206, name: "power", min_args: 1, max_args: Some(1), return_type: TypeId::Unknown },
    "round" => FunctionMetadata { id: 207, name: "round", min_args: 0, max_args: Some(1), return_type: TypeId::Unknown },
    "sqrt" => FunctionMetadata { id: 208, name: "sqrt", min_args: 0, max_args: Some(0), return_type: TypeId::Decimal },
    "truncate" => FunctionMetadata { id: 209, name: "truncate", min_args: 0, max_args: Some(0), return_type: TypeId::Integer },

    // Conversion functions
    "iif" => FunctionMetadata { id: 300, name: "iif", min_args: 2, max_args: Some(3), return_type: TypeId::Unknown },
    "toBoolean" => FunctionMetadata { id: 301, name: "toBoolean", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "convertsToBoolean" => FunctionMetadata { id: 302, name: "convertsToBoolean", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "toInteger" => FunctionMetadata { id: 303, name: "toInteger", min_args: 0, max_args: Some(0), return_type: TypeId::Integer },
    "convertsToInteger" => FunctionMetadata { id: 304, name: "convertsToInteger", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "toDecimal" => FunctionMetadata { id: 305, name: "toDecimal", min_args: 0, max_args: Some(0), return_type: TypeId::Decimal },
    "convertsToDecimal" => FunctionMetadata { id: 306, name: "convertsToDecimal", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "convertsToString" => FunctionMetadata { id: 307, name: "convertsToString", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "toDate" => FunctionMetadata { id: 308, name: "toDate", min_args: 0, max_args: Some(0), return_type: TypeId::Date },
    "convertsToDate" => FunctionMetadata { id: 309, name: "convertsToDate", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "toDateTime" => FunctionMetadata { id: 310, name: "toDateTime", min_args: 0, max_args: Some(0), return_type: TypeId::DateTime },
    "convertsToDateTime" => FunctionMetadata { id: 311, name: "convertsToDateTime", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "toTime" => FunctionMetadata { id: 312, name: "toTime", min_args: 0, max_args: Some(0), return_type: TypeId::Time },
    "convertsToTime" => FunctionMetadata { id: 313, name: "convertsToTime", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "toQuantity" => FunctionMetadata { id: 314, name: "toQuantity", min_args: 0, max_args: Some(0), return_type: TypeId::Quantity },
    "convertsToQuantity" => FunctionMetadata { id: 315, name: "convertsToQuantity", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },

    // Navigation functions
    "children" => FunctionMetadata { id: 400, name: "children", min_args: 0, max_args: Some(1), return_type: TypeId::Unknown },
    "descendants" => FunctionMetadata { id: 401, name: "descendants", min_args: 0, max_args: Some(1), return_type: TypeId::Unknown },

    // Type functions
    "is" => FunctionMetadata { id: 410, name: "is", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },

    // Utility functions
    "trace" => FunctionMetadata { id: 500, name: "trace", min_args: 1, max_args: Some(2), return_type: TypeId::Unknown },
    "now" => FunctionMetadata { id: 501, name: "now", min_args: 0, max_args: Some(0), return_type: TypeId::DateTime },
    "today" => FunctionMetadata { id: 502, name: "today", min_args: 0, max_args: Some(0), return_type: TypeId::Date },
    "timeOfDay" => FunctionMetadata { id: 503, name: "timeOfDay", min_args: 0, max_args: Some(0), return_type: TypeId::Time },
    "sort" => FunctionMetadata { id: 504, name: "sort", min_args: 0, max_args: Some(1), return_type: TypeId::Unknown },
    "lowBoundary" => FunctionMetadata { id: 505, name: "lowBoundary", min_args: 0, max_args: Some(1), return_type: TypeId::Unknown },
    "highBoundary" => FunctionMetadata { id: 506, name: "highBoundary", min_args: 0, max_args: Some(1), return_type: TypeId::Unknown },
    "comparable" => FunctionMetadata { id: 507, name: "comparable", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "precision" => FunctionMetadata { id: 508, name: "precision", min_args: 0, max_args: Some(0), return_type: TypeId::Integer },
    "type" => FunctionMetadata { id: 509, name: "type", min_args: 0, max_args: Some(0), return_type: TypeId::String },
    "conformsTo" => FunctionMetadata { id: 510, name: "conformsTo", min_args: 1, max_args: Some(1), return_type: TypeId::Boolean },
    "hasValue" => FunctionMetadata { id: 511, name: "hasValue", min_args: 0, max_args: Some(0), return_type: TypeId::Boolean },
    "resolve" => FunctionMetadata { id: 512, name: "resolve", min_args: 0, max_args: Some(0), return_type: TypeId::Unknown },

    // Aggregate functions
    "aggregate" => FunctionMetadata { id: 600, name: "aggregate", min_args: 2, max_args: Some(2), return_type: TypeId::Unknown },
};

/// Function registry
///
/// Provides fast function lookups using a compile-time perfect hash map.
/// The registry is now zero-allocation and provides O(1) lookups.
pub struct FunctionRegistry {
    functions_by_id: Vec<Option<FunctionMetadata>>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            functions_by_id: Vec::new(),
        };

        registry.build_id_index();
        registry
    }

    fn build_id_index(&mut self) {
        // Build the ID index from the static map
        // Find the maximum ID to size the vector correctly
        let max_id = FUNCTIONS_BY_NAME
            .values()
            .map(|m| m.id as usize)
            .max()
            .unwrap_or(0);

        self.functions_by_id.resize(max_id + 1, None);

        // Populate the ID index
        for metadata in FUNCTIONS_BY_NAME.values() {
            let id_value = metadata.id as usize;
            self.functions_by_id[id_value] = Some(*metadata);
        }
    }

    /// Resolve function name to FunctionId
    ///
    /// Uses a compile-time perfect hash map for O(1) lookup with zero allocation.
    pub fn resolve(&self, name: &str) -> Option<FunctionId> {
        FUNCTIONS_BY_NAME.get(name).map(|m| m.id)
    }

    /// Get function metadata by ID
    pub fn get_metadata(&self, id: FunctionId) -> Option<&FunctionMetadata> {
        let id_value = id as usize;
        self.functions_by_id.get(id_value)?.as_ref()
    }

    /// Validate function call arguments
    pub fn validate_args(&self, id: FunctionId, arg_count: usize) -> Result<(), String> {
        let metadata = self
            .get_metadata(id)
            .ok_or_else(|| format!("Function ID {} not found", id))?;

        if arg_count < metadata.min_args {
            return Err(format!(
                "Function {} requires at least {} arguments, got {}",
                metadata.name, metadata.min_args, arg_count
            ));
        }

        if let Some(max) = metadata.max_args {
            if arg_count > max {
                return Err(format!(
                    "Function {} takes at most {} arguments, got {}",
                    metadata.name, max, arg_count
                ));
            }
        }

        Ok(())
    }

    /// Get all registered function names (for testing/debugging)
    pub fn all_function_names(&self) -> Vec<&'static str> {
        FUNCTIONS_BY_NAME.keys().copied().collect()
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_functions_registered() {
        let registry = FunctionRegistry::new();

        // List of all FHIRPath functions
        let functions = vec![
            // Boolean logic
            "not",
            "as",
            // Conversion
            "iif",
            "toBoolean",
            "convertsToBoolean",
            "toInteger",
            "convertsToInteger",
            "toDecimal",
            "convertsToDecimal",
            "convertsToString",
            "toDate",
            "convertsToDate",
            "toDateTime",
            "convertsToDateTime",
            "toTime",
            "convertsToTime",
            "toQuantity",
            "convertsToQuantity",
            // Existence
            "empty",
            "exists",
            "all",
            "allTrue",
            "anyTrue",
            "allFalse",
            "anyFalse",
            "subsetOf",
            "supersetOf",
            "count",
            "distinct",
            "isDistinct",
            // Filtering
            "where",
            "select",
            "repeat",
            "ofType",
            "extension",
            // Subsetting
            "single",
            "first",
            "last",
            "tail",
            "skip",
            "take",
            "intersect",
            "exclude",
            // Combining
            "union",
            "combine",
            // String
            "indexOf",
            "lastIndexOf",
            "substring",
            "startsWith",
            "endsWith",
            "contains",
            "upper",
            "lower",
            "replace",
            "matches",
            "matchesFull",
            "replaceMatches",
            "length",
            "toChars",
            "toString",
            "encode",
            "decode",
            "escape",
            "unescape",
            "trim",
            "split",
            "join",
            // Math
            "abs",
            "ceiling",
            "exp",
            "floor",
            "ln",
            "log",
            "power",
            "round",
            "sqrt",
            "truncate",
            // Navigation
            "children",
            "descendants",
            // Type
            "is",
            // Utility
            "trace",
            "now",
            "today",
            "timeOfDay",
            "sort",
            "lowBoundary",
            "highBoundary",
            "comparable",
            "precision",
            "type",
            "conformsTo",
            "hasValue",
            "resolve",
            // Aggregate
            "aggregate",
        ];

        for func_name in functions {
            assert!(
                registry.resolve(func_name).is_some(),
                "Function '{}' is not registered",
                func_name
            );
        }
    }

    #[test]
    fn test_function_argument_validation() {
        let registry = FunctionRegistry::new();

        // Test valid calls
        assert!(registry.resolve("empty").is_some());
        let empty_id = registry.resolve("empty").unwrap();
        assert!(registry.validate_args(empty_id, 0).is_ok());

        // Test invalid argument count
        let where_id = registry.resolve("where").unwrap();
        assert!(registry.validate_args(where_id, 0).is_err());
        assert!(registry.validate_args(where_id, 1).is_ok());
        assert!(registry.validate_args(where_id, 2).is_err());

        // Test variable argument functions
        let round_id = registry.resolve("round").unwrap();
        assert!(registry.validate_args(round_id, 0).is_ok());
        assert!(registry.validate_args(round_id, 1).is_ok());
        assert!(registry.validate_args(round_id, 2).is_err());
    }
}
