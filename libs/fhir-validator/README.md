# fhir-validator

Flexible FHIR validation with a three-phase, pipeline-based architecture.

## Architecture

The validator cleanly separates configuration, planning, and execution, delegating all FHIR knowledge to `fhir-context`.

### Three-Phase Pipeline Design

```
ValidatorConfig → ValidationPlan → Validator → ValidationOutcome
  (declarative)     (executable)    (reusable)    (structured)
```

### Phase 1: Declarative Configuration

**Type**: `ValidatorConfig`

Defines *what* to validate and *how strict* to be:

- Preset-based (Ingestion, Authoring, Server, Publication)
- Fine-grained control per capability (Schema, Profiles, Constraints, Terminology, References, Bundles)
- Serializable (YAML/JSON)
- Builder pattern for ergonomic construction

**Key feature**: No combinatorial explosion. Capabilities are selected via configuration, not encoded as type parameters.

### Phase 2: Compiled Validation Plan

**Type**: `ValidationPlan`

Configuration compiles into an ordered, executable pipeline:

- Vector of `Step` enum variants (Schema, Profiles, Constraints, etc.)
- Validates configuration correctness (e.g., ReferenceMode::Full requires terminology)
- Eliminates disabled features
- Immutable after compilation

**Key feature**: Single validation pass at compile time ensures valid combinations.

### Phase 3: Reusable Validator

**Type**: `Validator<C: FhirContext>`

Owns the plan and FHIR context, reusable across many validations:

- Generic over context type (e.g., `DefaultFhirContext`)
- Heavy initialization (load packages, compile plan) done once
- Each `validate()` call creates short-lived `ValidationRun`
- Stateless execution - deterministic and independently testable

**Key feature**: Amortizes expensive setup across thousands of validations.

### Phase 4: Validation Execution

**Internal**: `ValidationRun<'a, C>`

Short-lived struct that executes the plan:

- Borrows plan, context, and resource
- Iterates through steps
- Accumulates `ValidationIssue`s
- Returns `ValidationOutcome`

**Key feature**: Zero allocation per-step, fail-fast support, issue limit enforcement.

### Validation Outcome

**Type**: `ValidationOutcome`

Structured validation result:

- Success/failure status
- List of `ValidationIssue` (severity, code, diagnostics, location, expression)
- Convertible to FHIR `OperationOutcome`
- Programmatically inspectable (error_count, warning_count, etc.)

**Key feature**: Structured output suitable for both human and machine consumption.

### FHIR Knowledge Delegation

All FHIR-specific knowledge lives in `fhir-context`:

- StructureDefinition resolution
- ValueSet/CodeSystem access
- Terminology service integration
- Reference resolution

The validator never hard-codes FHIR rules - it queries the context.

### Key Design Decisions

1. **Configuration separate from execution** - stable public API, internal optimization freedom
2. **Generic context** - testable with mock contexts, swappable implementations
3. **Stateless steps** - independently testable, parallelizable (future)
4. **Vector-based plan** - no type explosion, runtime composition
5. **Fail-fast + issue limits** - early termination for performance
6. **Structured outcomes** - programmatic inspection, FHIR-compliant output

### Extension Points

New validation capabilities added by:

1. Adding variant to `Step` enum
2. Adding corresponding config/plan types
3. Implementing step execution in `ValidationRun`
4. No changes to public `Validator` API

### Performance Characteristics

- **Setup**: O(packages) - load FHIR packages into context
- **Compilation**: O(1) - validate config, build step vector
- **Validation**: O(steps × resource_size) - linear in enabled steps and resource complexity
- **Memory**: Shared context across validations, minimal per-run allocation

## Usage

### Presets

```rust
use zunder_validator::{ValidatorConfig, Preset};

// Use a preset
let cfg = ValidatorConfig::preset(Preset::Server);

// Compile to executable plan
let plan = cfg.compile()?;
```

### Builder Pattern

```rust
let cfg = ValidatorConfig::builder()
    .preset(Preset::Server)
    .terminology_mode(TerminologyMode::Local)
    .fail_fast(true)
    .build();
```

### YAML Configuration

```rust
let yaml = r#"
preset: Server
terminology:
  mode: Local
  timeout: 2000
exec:
  fail_fast: true
"#;

let cfg = ValidatorConfig::from_yaml(yaml)?;
let plan = cfg.compile()?;
```

## Presets

- **Ingestion**: Fast structural validation only
- **Authoring**: Schema + profiles + constraints + local terminology
- **Server**: Production validation with hybrid terminology
- **Publication**: Strictest validation with remote terminology

## Configuration Options

### Schema
- `mode`: Off | On
- `allow_unknown_elements`: bool
- `allow_modifier_extensions`: bool

### Constraints
- `mode`: Off | InvariantsOnly | Full
- `best_practice`: Ignore | Warn | Error
- `suppress`: List of constraint IDs to skip
- `level_overrides`: Override severity levels

### Terminology
- `mode`: Off | Local | Remote | Hybrid
- `extensible_handling`: Ignore | Warn | Error
- `timeout`: Duration in milliseconds
- `on_timeout`: Skip | Warn | Error
- `cache`: None | Memory

### References
- `mode`: Off | TypeOnly | Existence | Full
- `allow_external`: bool

### Profiles
- `mode`: Off | On

### Bundles
- `mode`: Off | On

## Examples

See `examples/` for YAML configuration files for different use cases.
