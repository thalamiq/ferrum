# Validation Steps

Each validation step is implemented in its own module for clarity and testability.

## Schema Validation (`schema.rs`)

Validates resources against **base FHIR StructureDefinitions** (core resource types) using **expanded snapshots**.

### What it checks:
- ✓ `resourceType` field presence
- ✓ Base StructureDefinition lookup (e.g., `http://hl7.org/fhir/StructureDefinition/Patient`)
- ✓ Snapshot materialization (differential → snapshot via `baseDefinition`) and deep snapshot expansion (choice types, complex types, contentReference) via `fhir-snapshot`
- ✓ Element cardinality (min/max constraints)
- ✓ Primitive data type correctness
- ✓ Unknown elements (if `allow_unknown_elements: false`)
- ✓ Modifier extensions (if `allow_modifier_extensions: false`)

### Key Principle:
**Schema validates against BASE definitions only** - it ensures the resource conforms to the core FHIR specification for that resource type. It does NOT validate against profiles.

## Profile Validation (`profiles.rs`)

Validates resources against **constraining StructureDefinitions** (profiles) declared in `meta.profile` or `explicit_profiles`.

### What it checks:
- ✓ Profile StructureDefinition lookup from `fhir-context`
- ✓ Profile-specific cardinality constraints
- ✓ Fixed values and patterns
- ✓ **Slicing** (delegated to `slicing.rs` module)
- ✓ Type restrictions
- ✓ Must Support elements

### Key Principle:
**Profiles validate against PROFILE definitions** - they add additional constraints beyond the base schema. Slicing is a profile feature, so it's handled here (via the `slicing.rs` helper module).

## Constraints Validation (`constraints.rs`)

Evaluates **FHIRPath invariants** from both base resources and profiles.

### What it checks:
- ✓ FHIRPath constraint expressions (`constraint.expression`)
- ✓ Constraint severity levels
- ✓ Best practice constraints

### Key Principle:
**Constraints are separate** because invariants can exist in both base StructureDefinitions and profiles. This step validates all constraints regardless of source.

## Other Steps

- **Terminology** (`terminology.rs`) - Validate CodeableConcept/Coding bindings
- **References** (`references.rs`) - Validate Reference targets exist and have correct type
- **Bundles** (`bundles.rs`) - Validate Bundle-specific rules (transactions, uniqueness, etc.)

## Separation of Concerns

The validation steps follow a clear separation:

1. **Schema** → Validates against **base** StructureDefinitions (core FHIR resources)
2. **Profiles** → Validates against **profile** StructureDefinitions (constraining profiles), including slicing
3. **Constraints** → Validates **FHIRPath invariants** from both base and profiles

This separation makes it clear:
- Schema ensures basic structural correctness
- Profiles add domain-specific constraints (including slicing)
- Constraints validate business rules expressed as FHIRPath

## Integration

Each step exports a public `validate_*` function called from `validator.rs`:

```rust
pub fn validate_schema<C: FhirContext>(
    resource: &Value,
    plan: &SchemaPlan,
    context: &C,
    issues: &mut Vec<ValidationIssue>,
)
```

Steps are stateless - they only add issues to the provided vector.

For best performance across many validations, wrap your context once so expanded StructureDefinitions are cached:

```rust
use ferrum_validator::Validator;

let validator = Validator::new(plan, context).with_expanded_snapshots();
```
