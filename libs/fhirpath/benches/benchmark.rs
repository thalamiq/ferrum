//! Criterion benchmarks for FHIRPath engine performance

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::time::Duration;
use ferrum_fhirpath::{Context, Engine, Value};

fn create_test_engine() -> Engine {
    tokio::runtime::Runtime::new()
        .expect("failed to create Tokio runtime")
        .block_on(Engine::with_fhir_version("R5"))
        .unwrap_or_else(|e| panic!("failed to create test engine: {}", e))
}

fn custom_criterion() -> Criterion {
    Criterion::default()
        .sample_size(20) // Reduced from default 100
        .warm_up_time(Duration::from_millis(100)) // Reduced warmup
        .measurement_time(Duration::from_secs(1)) // Reduced measurement time
        .nresamples(1000) // Reduced from default 100000
        .noise_threshold(0.05) // Slightly higher threshold for faster convergence
}

fn bench_simple_arithmetic(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("simple_arithmetic", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("1 + 2 * 3"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_string_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("string_length", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'hello world'.length()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_upper", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'hello'.upper()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_substring", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'hello world'.substring(0, 5)"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_collection_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("collection_union", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3 | 4 | 5)"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("collection_where", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("(1 | 2 | 3 | 4 | 5).where($this > 2)"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    c.bench_function("collection_select", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("(1 | 2 | 3 | 4 | 5).select($this * 2)"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });
}

fn bench_complex_expressions(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("complex_expression", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("(1 | 2 | 3).where($this > 1).select($this * 2).first()"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });
}

fn bench_compilation_cache(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    // First evaluation (compilation)
    engine.evaluate_expr("1 + 2", &ctx, None).unwrap();

    c.bench_function("cached_expression", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("1 + 2"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_large_collections(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    // Create expression with large collection
    let mut expr = "1".to_string();
    for i in 2..=100 {
        expr = format!("{} | {}", expr, i);
    }

    c.bench_function("large_collection", |b| {
        b.iter(|| engine.evaluate_expr(black_box(&expr), &ctx, None).unwrap())
    });
}

fn bench_nested_expressions(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    // Create deeply nested expression
    let mut expr = "1".to_string();
    for i in 2..=50 {
        expr = format!("({} + {})", expr, i);
    }

    c.bench_function("nested_expression", |b| {
        b.iter(|| engine.evaluate_expr(black_box(&expr), &ctx, None).unwrap())
    });
}

fn bench_type_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("type_is", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("1 is Integer"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("type_as", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("1 as Integer"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_usecontext_code_expressions(c: &mut Criterion) {
    let engine = create_test_engine();

    // Helper to create a resource with useContext
    let create_resource_with_usecontext = |resource_type: &str| -> Value {
        let resource_json = json!({
            "resourceType": resource_type,
            "id": format!("test-{}", resource_type.to_lowercase()),
            "useContext": [
                {
                    "code": {
                        "system": "http://terminology.hl7.org/CodeSystem/usage-context-type",
                        "code": "focus",
                        "display": "Clinical Focus"
                    },
                    "valueCodeableConcept": {
                        "coding": [
                            {
                                "system": "http://snomed.info/sct",
                                "code": "182836005",
                                "display": "Review of medication"
                            }
                        ]
                    }
                },
                {
                    "code": {
                        "system": "http://terminology.hl7.org/CodeSystem/usage-context-type",
                        "code": "user",
                        "display": "User Type"
                    },
                    "valueCodeableConcept": {
                        "coding": [
                            {
                                "system": "http://terminology.hl7.org/CodeSystem/v3-role-class",
                                "code": "PROV",
                                "display": "healthcare provider"
                            }
                        ]
                    }
                }
            ]
        });
        Value::from_json(resource_json)
    };

    // Benchmark individual resource types
    let resource_types = [
        "CapabilityStatement",
        "CodeSystem",
        "ConceptMap",
        "OperationDefinition",
        "SearchParameter",
        "StructureDefinition",
        "ValueSet",
    ];

    for resource_type in &resource_types {
        let resource = create_resource_with_usecontext(resource_type);
        let ctx = Context::new(resource);
        let expr = format!("{}.useContext.code", resource_type);

        c.bench_function(
            &format!("usecontext_code_{}", resource_type.to_lowercase()),
            |b| b.iter(|| engine.evaluate_expr(black_box(&expr), &ctx, None).unwrap()),
        );
    }

    // Benchmark the complex union expression used in search parameters
    let capability_statement = create_resource_with_usecontext("CapabilityStatement");
    let ctx = Context::new(capability_statement);

    let union_expr = "CapabilityStatement.useContext.code | CodeSystem.useContext.code | CompartmentDefinition.useContext.code | ConceptMap.useContext.code | GraphDefinition.useContext.code | ImplementationGuide.useContext.code | MessageDefinition.useContext.code | NamingSystem.useContext.code | OperationDefinition.useContext.code | SearchParameter.useContext.code | StructureDefinition.useContext.code | StructureMap.useContext.code | TerminologyCapabilities.useContext.code | ValueSet.useContext.code";

    c.bench_function("usecontext_code_union_all_types", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box(union_expr), &ctx, None)
                .unwrap()
        })
    });

    // Benchmark with multiple resources to test union behavior
    let value_set = create_resource_with_usecontext("ValueSet");
    let ctx_value_set = Context::new(value_set);

    c.bench_function("usecontext_code_union_value_set", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box(union_expr), &ctx_value_set, None)
                .unwrap()
        })
    });

    // Benchmark nested path: useContext.code.coding.code
    let structure_def = create_resource_with_usecontext("StructureDefinition");
    let ctx_sd = Context::new(structure_def);

    c.bench_function("usecontext_code_coding_code", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("StructureDefinition.useContext.code.coding.code"),
                    &ctx_sd,
                    None,
                )
                .unwrap()
        })
    });

    // Benchmark with where clause filtering
    c.bench_function("usecontext_code_where_filter", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box(
                        "StructureDefinition.useContext.where(code.coding.code = 'focus').code",
                    ),
                    &ctx_sd,
                    None,
                )
                .unwrap()
        })
    });
}

fn bench_boolean_logical_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("boolean_and", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("true and false"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("boolean_or", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("true or false"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("boolean_not", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("true.not()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("comparison_equals", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("5 = 5"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("comparison_not_equals", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("5 != 3"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("comparison_less_than", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("3 < 5"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("comparison_greater_than", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("5 > 3"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("membership_in", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("3 in (1 | 2 | 3 | 4 | 5)"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("membership_contains", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3 | 4 | 5) contains 3"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_date_time_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("date_comparison", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("@2024-01-01 < @2024-12-31"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("datetime_comparison", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("@2024-01-01T10:00:00 < @2024-01-01T11:00:00"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    c.bench_function("time_comparison", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("@T10:00:00 < @T11:00:00"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("now_function", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("now()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("today_function", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("today()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("time_of_day_function", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("timeOfDay()"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_mathematical_functions(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("math_abs", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(-5).abs()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("math_ceil", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("5.7.ceiling()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("math_floor", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("5.7.floor()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("math_round", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("5.7.round()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("math_sqrt", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("16.sqrt()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("math_power", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("2.power(8)"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("math_truncate", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("5.7.truncate()"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_advanced_string_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("string_contains", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'hello world'.contains('world')"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_starts_with", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'hello world'.startsWith('hello')"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_ends_with", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'hello world'.endsWith('world')"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_index_of", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'hello world'.indexOf('world')"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_replace", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("'hello world'.replace('world', 'universe')"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    c.bench_function("string_matches", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'hello123'.matches('\\d+')"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_trim", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'  hello world  '.trim()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_split", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'a,b,c'.split(',')"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("string_join", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("('a' | 'b' | 'c').join(',')"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_aggregation_functions(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    // Sum using aggregate
    c.bench_function("aggregate_sum", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("(1 | 2 | 3 | 4 | 5).aggregate($this + $total, 0)"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    // Count
    c.bench_function("aggregate_count", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3 | 4 | 5).count()"), &ctx, None)
                .unwrap()
        })
    });

    // Min using aggregate
    c.bench_function("aggregate_min", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box(
                        "(5 | 2 | 8 | 1 | 9).aggregate(iif($total.empty(), $this, iif($this < $total, $this, $total)))",
                    ),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    // Average (sum / count)
    c.bench_function("aggregate_avg", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box(
                        "(1 | 2 | 3 | 4 | 5).aggregate($total + $this, 0) / (1 | 2 | 3 | 4 | 5).count()",
                    ),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });
}

fn bench_existence_and_empty_checks(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("existence_empty", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3).empty()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("existence_exists", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3).exists()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("existence_exists_with_predicate", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3).exists($this > 2)"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("existence_all", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3).all($this > 0)"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("existence_all_true", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(true | true | true).allTrue()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("existence_any_true", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(false | true | false).anyTrue()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("existence_distinct", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 2 | 3 | 3 | 3).distinct()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("existence_is_distinct", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3).isDistinct()"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_subsetting_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("subsetting_first", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3 | 4 | 5).first()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("subsetting_last", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3 | 4 | 5).last()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("subsetting_tail", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3 | 4 | 5).tail()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("subsetting_skip", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3 | 4 | 5).skip(2)"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("subsetting_take", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(1 | 2 | 3 | 4 | 5).take(3)"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("subsetting_intersect", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("(1 | 2 | 3 | 4 | 5).intersect((3 | 4 | 5 | 6 | 7))"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    c.bench_function("subsetting_exclude", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("(1 | 2 | 3 | 4 | 5).exclude((3 | 4))"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    c.bench_function("subsetting_single", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("(42).single()"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_navigation_operations(c: &mut Criterion) {
    let engine = create_test_engine();

    // Create a Patient resource for navigation benchmarks
    let patient_json = json!({
        "resourceType": "Patient",
        "id": "example",
        "name": [
            {
                "family": "Doe",
                "given": ["John", "Michael"]
            },
            {
                "family": "Smith",
                "given": ["Jane"]
            }
        ],
        "telecom": [
            {
                "system": "phone",
                "value": "555-1234"
            },
            {
                "system": "email",
                "value": "john@example.com"
            }
        ],
        "address": [
            {
                "line": ["123 Main St"],
                "city": "Anytown",
                "state": "CA",
                "postalCode": "12345"
            }
        ]
    });
    let patient = Value::from_json(patient_json);
    let ctx = Context::new(patient);

    c.bench_function("navigation_simple_path", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Patient.name"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("navigation_nested_path", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Patient.name.family"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("navigation_deep_path", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Patient.address.line"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("navigation_children", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Patient.name.children()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("navigation_descendants", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Patient.descendants()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("navigation_where_filter", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("Patient.telecom.where(system = 'email')"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });
}

fn bench_conditional_expressions(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("conditional_iif_simple", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("iif(true, 'yes', 'no')"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("conditional_iif_complex", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("iif(5 > 3, 'greater', 'less or equal')"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    c.bench_function("conditional_chained", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("iif(5 > 10, 'high', iif(5 > 3, 'medium', 'low'))"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });
}

fn bench_conversion_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("conversion_to_string", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("42.toString()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("conversion_to_integer", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'42'.toInteger()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("conversion_to_decimal", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'3.14'.toDecimal()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("conversion_to_boolean", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'true'.toBoolean()"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("conversion_converts_to_integer", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'42'.convertsToInteger()"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_complex_fhir_resource_expressions(c: &mut Criterion) {
    let engine = create_test_engine();

    // Create an Observation resource
    let observation_json = json!({
        "resourceType": "Observation",
        "id": "example",
        "status": "final",
        "code": {
            "coding": [
                {
                    "system": "http://loinc.org",
                    "code": "718-7",
                    "display": "Hemoglobin"
                }
            ]
        },
        "subject": {
            "reference": "Patient/example"
        },
        "valueQuantity": {
            "value": 14.5,
            "unit": "g/dL",
            "system": "http://unitsofmeasure.org",
            "code": "g/dL"
        },
        "component": [
            {
                "code": {
                    "coding": [
                        {
                            "system": "http://loinc.org",
                            "code": "718-7",
                            "display": "Hemoglobin"
                        }
                    ]
                },
                "valueQuantity": {
                    "value": 14.5,
                    "unit": "g/dL"
                }
            }
        ]
    });
    let observation = Value::from_json(observation_json);
    let ctx = Context::new(observation);

    c.bench_function("fhir_observation_code", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Observation.code"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("fhir_observation_code_coding_code", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Observation.code.coding.code"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("fhir_observation_value_quantity", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Observation.valueQuantity"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("fhir_observation_value_quantity_value", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Observation.valueQuantity.value"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("fhir_observation_component_filter", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(
                    black_box("Observation.component.where(code.coding.code = '718-7')"),
                    &ctx,
                    None,
                )
                .unwrap()
        })
    });

    c.bench_function("fhir_observation_of_type", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("Observation.value.ofType(Quantity)"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_equivalence_operations(c: &mut Criterion) {
    let engine = create_test_engine();
    let ctx = Context::new(Value::empty());

    c.bench_function("equivalence_tilde", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'Hello' ~ 'hello'"), &ctx, None)
                .unwrap()
        })
    });

    c.bench_function("equivalence_not_tilde", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box("'Hello' !~ 'world'"), &ctx, None)
                .unwrap()
        })
    });
}

fn bench_check_digit_validation(c: &mut Criterion) {
    let engine = create_test_engine();

    // Create context with a valid 13-digit identifier (e.g., EAN-13: 9780201379624)
    let identifier = Value::string("9780201379624".to_string());
    let ctx = Context::new(identifier);

    // Complex check digit validation expression using substring and modulo operations
    let check_digit_expr = "((10-((substring(0,1).toInteger()*1)+(substring(1,1).toInteger()*3)+(substring(2,1).toInteger()*1)+(substring(3,1).toInteger()*3)+(substring(4,1).toInteger()*1)+(substring(5,1).toInteger()*3)+(substring(6,1).toInteger()*1)+(substring(7,1).toInteger()*3)+(substring(8,1).toInteger()*1)+(substring(9,1).toInteger()*3)+(substring(10,1).toInteger()*1)+(substring(11,1).toInteger()*3))mod(10))mod(10))=substring(12,1).toInteger()";

    c.bench_function("check_digit_validation", |b| {
        b.iter(|| {
            engine
                .evaluate_expr(black_box(check_digit_expr), &ctx, None)
                .unwrap()
        })
    });
}

criterion_group! {
    name = benches;
    config = custom_criterion();
    targets =
        bench_simple_arithmetic,
        bench_string_operations,
        bench_collection_operations,
        bench_complex_expressions,
        bench_compilation_cache,
        bench_large_collections,
        bench_nested_expressions,
        bench_type_operations,
        bench_usecontext_code_expressions,
        bench_boolean_logical_operations,
        bench_date_time_operations,
        bench_mathematical_functions,
        bench_advanced_string_operations,
        bench_aggregation_functions,
        bench_existence_and_empty_checks,
        bench_subsetting_operations,
        bench_navigation_operations,
        bench_conditional_expressions,
        bench_conversion_operations,
        bench_complex_fhir_resource_expressions,
        bench_equivalence_operations,
        bench_check_digit_validation
}
criterion_main!(benches);
