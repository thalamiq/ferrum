//! Main FHIRPath engine
//!
//! Orchestrates the compilation pipeline: Parse → AST → HIR → VM Plan → Execution

use crate::analyzer::{self, Analyzer};
use crate::codegen::CodeGenerator;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::functions::FunctionRegistry;
use crate::resolver::ResourceResolver;
use crate::types::TypeRegistry;
use crate::value::{Collection, Value};
use crate::variables::VariableRegistry;
use crate::vm::Plan;
use lru::LruCache;
use std::sync::{Arc, Mutex};
use zunder_context::{DefaultFhirContext, FhirContext};

#[derive(Clone, Debug, Default)]
pub struct CompileOptions {
    /// Optional base type name used for semantic type annotation and (when `strict`)
    /// StructureDefinition-based path validation.
    pub base_type: Option<String>,
    /// If `true`, invalid path navigation on resolvable FHIR types errors at compile time.
    pub strict: bool,
}

#[derive(Clone, Debug)]
pub struct EvalOptions {
    /// Optional base type name used for compile-time typing/validation.
    pub base_type: Option<String>,
    /// If `true`, enables compile-time strict validation (independent of `base_type` presence).
    pub strict: bool,
    /// If `true` and `base_type` is not provided, attempt to infer a base type from the
    /// runtime resource (`resourceType`) for relative paths (e.g., `name.given`).
    pub infer_base_type: bool,
}

impl Default for EvalOptions {
    fn default() -> Self {
        Self {
            base_type: None,
            strict: false,
            infer_base_type: true,
        }
    }
}

/// Main FHIRPath engine
///
/// Requires a FHIR context for runtime type resolution from StructureDefinitions.
/// The context is used during AST → HIR compilation to infer types for path navigation.
///
/// Optionally accepts a custom ResourceResolver for overriding the `resolve()` function
/// behavior. This is useful for database-backed resolution or other custom logic.
pub struct Engine {
    type_registry: Arc<TypeRegistry>,
    function_registry: Arc<FunctionRegistry>,
    cache: Arc<Mutex<LruCache<String, Arc<Plan>>>>,
    variable_registry: Arc<Mutex<VariableRegistry>>,
    fhir_context: Arc<dyn FhirContext>,
    resource_resolver: Option<Arc<dyn ResourceResolver>>,
}

impl Engine {
    /// Create a new engine with FHIR context and custom ResourceResolver
    ///
    /// The FHIR context provides StructureDefinition lookup for type inference during
    /// HIR generation. This allows the engine to work with any Implementation Guide
    /// without requiring static compilation of all FHIR types.
    ///
    /// The custom resolver will be used by the `resolve()` function to resolve
    /// FHIR references. This is useful for database-backed resolution or other
    /// custom logic.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use fhirpath_engine::{Engine, ResourceResolver};
    /// use std::sync::Arc;
    ///
    /// struct MyResolver { /* ... */ }
    /// impl ResourceResolver for MyResolver { /* ... */ }
    ///
    /// let resolver = Arc::new(MyResolver::new());
    /// let engine = Engine::new_with_resolver(fhir_context, Some(resolver));
    /// ```
    pub fn new(context: Arc<dyn FhirContext>, resolver: Option<Arc<dyn ResourceResolver>>) -> Self {
        Self {
            type_registry: Arc::new(TypeRegistry::new()),
            function_registry: Arc::new(FunctionRegistry::new()),
            cache: Arc::new(Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(1000).unwrap(),
            ))),
            variable_registry: Arc::new(Mutex::new(VariableRegistry::new())),
            fhir_context: context,
            resource_resolver: resolver,
        }
    }

    /// Create an engine with a default FHIR context loaded from registry cache (async).
    ///
    /// The engine will attempt to load the base FHIR package for the specified version
    /// from the registry cache (~/.fhir/packages/). If the package is not found in cache,
    /// it will download from Simplifier.
    ///
    /// Supported versions: "R4", "R4B", "R5"
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use zunder_fhirpath::Engine;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// // Load R5 context from cache or download
    /// let engine = Engine::with_fhir_version("R5").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_fhir_version(version: &str) -> Result<Self> {
        let context: Arc<dyn FhirContext> = Arc::new(
            DefaultFhirContext::from_fhir_version_async(None, version)
                .await
                .map_err(|e| {
                    Error::EvaluationError(format!(
                        "Failed to load FHIR package {}: {}",
                        version, e
                    ))
                })?,
        );

        Ok(Self::new(context, None))
    }

    pub fn fhir_context(&self) -> &Arc<dyn FhirContext> {
        &self.fhir_context
    }

    /// Get the custom resource resolver (if any)
    pub fn resource_resolver(&self) -> Option<&Arc<dyn ResourceResolver>> {
        self.resource_resolver.as_ref()
    }

    // ============================================================================
    // Compilation
    // ============================================================================

    /// Compile a FHIRPath expression to a VM plan.
    ///
    /// Optionally accepts a base type name for strict validation. When provided
    /// (e.g., `Some("Patient")`), the compiler will validate that all field accesses
    /// are valid according to the StructureDefinition.
    ///
    /// For explicit (decoupled) strictness and base typing, use `compile_with_options()`.
    ///
    /// If the type is not found in the FHIR context or type registry, compilation
    /// will still proceed but with reduced type validation (the analyzer will use
    /// a fallback type). This allows compilation to work even with empty contexts
    /// or unknown types, which is useful for testing or when working without
    /// full FHIR package definitions.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use zunder_fhirpath::Engine;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = Engine::with_fhir_version("R5").await?;
    ///
    /// // Compile without type validation
    /// let plan1 = engine.compile("Patient.name.given", None)?;
    ///
    /// // Compile with type validation
    /// let plan2 = engine.compile("name.given", Some("Patient"))?;
    /// # Ok(()) }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn compile(&self, expr: &str, base_type: Option<&str>) -> Result<Arc<Plan>> {
        // Backwards-compatible convenience API: providing a base type implies strict validation.
        self.compile_with_options(
            expr,
            CompileOptions {
                base_type: base_type.map(|s| s.to_string()),
                strict: base_type.is_some(),
            },
        )
    }

    /// Compile a FHIRPath expression to a VM plan with explicit options.
    pub fn compile_with_options(&self, expr: &str, options: CompileOptions) -> Result<Arc<Plan>> {
        self.compile_internal(expr, &options)
    }

    fn leading_identifier(expr: &str) -> Option<&str> {
        let s = expr.trim_start();
        let mut chars = s.char_indices();
        let (start, first) = chars.next()?;
        debug_assert_eq!(start, 0);
        if !(first.is_ascii_alphabetic() || first == '_') {
            return None;
        }
        let mut end = first.len_utf8();
        for (idx, c) in chars {
            if c.is_ascii_alphanumeric() || c == '_' {
                end = idx + c.len_utf8();
            } else {
                break;
            }
        }
        Some(&s[..end])
    }

    fn resource_type_from_value(resource: &Value) -> Option<String> {
        match resource.data() {
            crate::value::ValueData::LazyJson { .. } => match resource.data().resolved_json()? {
                serde_json::Value::Object(obj) => obj
                    .get("resourceType")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        crate::vm::infer_structural_root_type_name_from_json(obj)
                            .map(|s| s.to_string())
                    }),
                _ => None,
            },
            crate::value::ValueData::Object(obj_map) => obj_map
                .get("resourceType")
                .and_then(|col| col.iter().next())
                .and_then(|value| match value.data() {
                    crate::value::ValueData::String(rt) => Some(rt.as_ref().to_string()),
                    _ => None,
                })
                .or_else(|| {
                    crate::vm::infer_structural_root_type_name(obj_map.as_ref())
                        .map(|s| s.to_string())
                }),
            _ => None,
        }
    }

    fn infer_compile_base_type_for_eval(
        &self,
        expr: &str,
        ctx: &Context,
        options: &EvalOptions,
    ) -> Option<String> {
        if options.base_type.is_some() || !options.infer_base_type {
            return options.base_type.clone();
        }

        let rt = Self::resource_type_from_value(&ctx.resource)?;

        // If the expression already starts with a known FHIR type name, don't lock compilation to
        // the runtime resource type (typing can be inferred from the expression itself).
        if let Some(ident) = Self::leading_identifier(expr) {
            if analyzer::is_fhir_type(&self.fhir_context, ident) || ident.eq_ignore_ascii_case(&rt)
            {
                return None;
            }
        }

        Some(rt)
    }

    /// Internal compilation method with explicit options.
    fn compile_internal(&self, expr: &str, options: &CompileOptions) -> Result<Arc<Plan>> {
        let cache_key = if options.strict {
            if let Some(base) = options.base_type.as_deref() {
                format!("strict:{}::{}", base, expr)
            } else {
                format!("strict::{}", expr)
            }
        } else {
            format!("lenient::{}", expr)
        };

        // Check cache first
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(plan) = cache.get(&cache_key) {
                return Ok(plan.clone());
            }
        }

        // 1. Parse → AST
        let mut parser = crate::parser::Parser::new(expr.to_string());
        let ast = parser.parse()?;

        // Determine a typing/validation base type:
        // - explicitly provided `base_type`
        // - otherwise inferred from a leading type name prefix in the expression (e.g., `Patient.name`)
        let base_type_from_expr = Self::leading_identifier(expr)
            .filter(|ident| analyzer::is_fhir_type(&self.fhir_context, ident))
            .map(|s| s.to_string());
        let typing_base_type = options.base_type.clone().or(base_type_from_expr);

        if options.strict && typing_base_type.is_none() {
            return Err(Error::TypeError(
                "Strict compilation requires a base type (provide one or prefix the expression with a root type)".into(),
            ));
        }

        // 2. Semantic analysis → HIR (structural only, for now still includes type resolution)
        let analyzer = Analyzer::new(
            Arc::clone(&self.type_registry),
            Arc::clone(&self.function_registry),
            Arc::clone(&self.variable_registry),
        );
        let hir = analyzer.analyze_with_type(ast, typing_base_type.clone())?;

        // 3. Type resolution pass (NEW TWO-PHASE ARCHITECTURE)
        // For now, this is optional and runs after the analyzer
        // TODO: Update Analyzer to produce Unknown types, making this pass mandatory
        let type_pass = crate::typecheck::TypePass::new(
            Arc::clone(&self.type_registry),
            Arc::clone(&self.function_registry),
            Arc::clone(&self.fhir_context),
        );
        let hir = type_pass.resolve(hir, typing_base_type, options.strict)?;

        // 4. Generate VM plan
        let plan = self.codegen(hir)?;
        let plan = Arc::new(plan);

        // Cache the plan
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(cache_key, plan.clone());
        }

        Ok(plan)
    }

    // ============================================================================
    // Evaluation
    // ============================================================================

    /// Evaluate a compiled plan against a context.
    pub fn evaluate(&self, plan: &Plan, ctx: &Context) -> Result<Collection> {
        use crate::vm::Vm;
        let mut vm = Vm::new(ctx, self);
        vm.execute(plan)
    }

    /// Evaluate a compiled plan against multiple JSON string resources in batch.
    ///
    /// OPTIMIZED: Accepts JSON strings directly to avoid double serialization.
    /// This is highly optimized for bulk operations:
    /// - Single FFI boundary crossing instead of N calls
    /// - Avoids JSON serialization overhead (resources already strings)
    /// - Tight loop in Rust for better CPU cache utilization
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let plan = engine.compile("Patient.name.given")?;
    /// let json_strings = vec!["{\"resourceType\":\"Patient\"...}", ...];
    /// let results = engine.evaluate_batch(&plan, &json_strings)?;
    /// ```
    pub fn evaluate_batch(&self, plan: &Plan, json_strings: &[&str]) -> Result<Vec<Collection>> {
        use crate::vm::Vm;

        // Pre-allocate result vector
        let mut results = Vec::with_capacity(json_strings.len());

        // Pre-parse all JSON strings to avoid repeated parsing overhead
        let mut parsed_resources = Vec::with_capacity(json_strings.len());
        for json_str in json_strings {
            let json_value: serde_json::Value = serde_json::from_str(json_str)
                .map_err(|e| Error::EvaluationError(format!("Invalid JSON: {}", e)))?;
            parsed_resources.push(json_value);
        }

        // Evaluate each resource in a tight loop
        // This stays in Rust, avoiding FFI overhead for each resource
        for resource in parsed_resources {
            let root = Value::from_json(resource);
            let ctx = Context::new(root);
            let mut vm = Vm::new(&ctx, self);
            let collection = vm.execute(plan)?;
            results.push(collection);
        }

        Ok(results)
    }

    /// Evaluate an expression directly (compile + evaluate).
    ///
    /// Optionally accepts a base type name for strict validation during compilation.
    ///
    /// For explicit (decoupled) strictness and base typing, use `evaluate_expr_with_options()`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use zunder_fhirpath::{Engine, Context, Value};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = Engine::with_fhir_version("R5").await?;
    /// let ctx = Context::new(Value::empty());
    ///
    /// // Evaluate without type validation
    /// let result1 = engine.evaluate_expr("1 + 2", &ctx, None)?;
    ///
    /// // Evaluate with type validation
    /// let result2 = engine.evaluate_expr("name.given", &ctx, Some("Patient"))?;
    /// # Ok(()) }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn evaluate_expr(
        &self,
        expr: &str,
        ctx: &Context,
        base_type: Option<&str>,
    ) -> Result<Collection> {
        // Backwards-compatible convenience API: providing a base type implies strict validation.
        self.evaluate_expr_with_options(
            expr,
            ctx,
            EvalOptions {
                base_type: base_type.map(|s| s.to_string()),
                strict: base_type.is_some(),
                infer_base_type: true,
            },
        )
    }

    /// Evaluate an expression directly (compile + evaluate) with explicit options.
    pub fn evaluate_expr_with_options(
        &self,
        expr: &str,
        ctx: &Context,
        options: EvalOptions,
    ) -> Result<Collection> {
        let inferred_base = self.infer_compile_base_type_for_eval(expr, ctx, &options);
        let plan = self.compile_with_options(
            expr,
            CompileOptions {
                base_type: inferred_base.or(options.base_type),
                strict: options.strict,
            },
        )?;
        self.evaluate(&plan, ctx)
    }

    /// Evaluate an expression against a JSON resource.
    ///
    /// Optionally accepts a base type name for strict validation during compilation.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use zunder_fhirpath::Engine;
    /// use serde_json::json;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = Engine::with_fhir_version("R5").await?;
    /// let resource = json!({"resourceType": "Patient", "name": [{"given": ["John"]}]});
    ///
    /// // Evaluate without type validation
    /// let result1 = engine.evaluate_json("Patient.name.given", &resource, None)?;
    ///
    /// // Evaluate with type validation
    /// let result2 = engine.evaluate_json("name.given", &resource, Some("Patient"))?;
    /// # Ok(()) }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn evaluate_json(
        &self,
        expr: &str,
        resource: serde_json::Value,
        base_type: Option<&str>,
    ) -> Result<Collection> {
        let root = Value::from_json(resource);
        let ctx = Context::new(root);
        self.evaluate_expr(expr, &ctx, base_type)
    }

    /// Evaluate an expression against an XML resource string.
    ///
    /// This method converts the XML resource to JSON internally before evaluation.
    /// The XML must be a valid FHIR resource in XML format.
    ///
    /// Optionally accepts a base type name for strict validation during compilation.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use zunder_fhirpath::Engine;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = Engine::with_fhir_version("R5").await?;
    /// let xml = r#"<Patient xmlns="http://hl7.org/fhir">
    ///     <id value="pat-1"/>
    ///     <active value="true"/>
    /// </Patient>"#;
    ///
    /// // Evaluate without type validation
    /// let result1 = engine.evaluate_xml("Patient.active", xml, None)?;
    ///
    /// // Evaluate with type validation
    /// let result2 = engine.evaluate_xml("active", xml, Some("Patient"))?;
    /// # Ok(()) }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[cfg(feature = "xml-support")]
    pub fn evaluate_xml(
        &self,
        expr: &str,
        xml_resource: &str,
        base_type: Option<&str>,
    ) -> Result<Collection> {
        let json_str = fhir_format::xml_to_json(xml_resource)
            .map_err(|e| Error::EvaluationError(format!("XML parse error: {}", e)))?;
        let resource: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| Error::EvaluationError(format!("JSON parse error: {}", e)))?;
        self.evaluate_json(expr, resource, base_type)
    }

    /// Evaluate an expression against a Value.
    ///
    /// Optionally accepts a base type name for strict validation during compilation.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use zunder_fhirpath::{Engine, Value};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = Engine::with_fhir_version("R5").await?;
    /// let resource = Value::empty();
    ///
    /// // Evaluate without type validation
    /// let result1 = engine.evaluate_value("1 + 2", resource.clone(), None)?;
    ///
    /// // Evaluate with type validation
    /// let result2 = engine.evaluate_value("name.given", resource, Some("Patient"))?;
    /// # Ok(()) }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn evaluate_value(
        &self,
        expr: &str,
        resource: Value,
        base_type: Option<&str>,
    ) -> Result<Collection> {
        let ctx = Context::new(resource);
        self.evaluate_expr(expr, &ctx, base_type)
    }

    /// Check if a type name is a FHIR type (not a System type)
    pub fn is_fhir_type(&self, type_name: &str) -> bool {
        analyzer::is_fhir_type(&self.fhir_context, type_name)
    }

    // ============================================================================
    // Visualization
    // ============================================================================

    /// Visualize the compilation pipeline for an expression
    ///
    /// Returns AST, HIR, and VM Plan visualizations in the specified format.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use fhirpath_engine::{Engine, visualize::VisualizationFormat};
    ///
    /// let engine = Engine::with_empty_context();
    /// let viz = engine.visualize_pipeline("Patient.name", VisualizationFormat::AsciiTree)?;
    /// println!("AST:\n{}", viz.ast);
    /// println!("HIR:\n{}", viz.hir);
    /// println!("Plan:\n{}", viz.plan);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn visualize_pipeline(
        &self,
        expr: &str,
        format: crate::visualize::VisualizationFormat,
    ) -> Result<PipelineVisualization> {
        use crate::visualize::Visualize;

        // 1. Parse → AST
        let mut parser = crate::parser::Parser::new(expr.to_string());
        let ast = parser.parse()?;
        let ast_viz = ast.visualize(format);

        // 2. Analyze → HIR
        let analyzer = Analyzer::new(
            Arc::clone(&self.type_registry),
            Arc::clone(&self.function_registry),
            Arc::clone(&self.variable_registry),
        );
        let hir = analyzer.analyze_with_type(ast, None)?;

        // 3. Type resolution
        let type_pass = crate::typecheck::TypePass::new(
            Arc::clone(&self.type_registry),
            Arc::clone(&self.function_registry),
            Arc::clone(&self.fhir_context),
        );
        let hir = type_pass.resolve(hir, None, false)?;
        let hir_viz = hir.visualize(format);

        // 4. Codegen
        let plan = self.codegen(hir)?;
        let plan_viz = plan.visualize(format);

        Ok(PipelineVisualization {
            ast: ast_viz,
            hir: hir_viz,
            plan: plan_viz,
        })
    }

    /// Visualize just the AST for an expression
    pub fn visualize_ast(
        &self,
        expr: &str,
        format: crate::visualize::VisualizationFormat,
    ) -> Result<String> {
        use crate::visualize::Visualize;
        let mut parser = crate::parser::Parser::new(expr.to_string());
        let ast = parser.parse()?;
        Ok(ast.visualize(format))
    }

    /// Visualize just the HIR for an expression
    pub fn visualize_hir(
        &self,
        expr: &str,
        format: crate::visualize::VisualizationFormat,
    ) -> Result<String> {
        use crate::visualize::Visualize;
        let mut parser = crate::parser::Parser::new(expr.to_string());
        let ast = parser.parse()?;
        let analyzer = Analyzer::new(
            Arc::clone(&self.type_registry),
            Arc::clone(&self.function_registry),
            Arc::clone(&self.variable_registry),
        );
        let hir = analyzer.analyze_with_type(ast, None)?;
        let type_pass = crate::typecheck::TypePass::new(
            Arc::clone(&self.type_registry),
            Arc::clone(&self.function_registry),
            Arc::clone(&self.fhir_context),
        );
        let hir = type_pass.resolve(hir, None, false)?;
        Ok(hir.visualize(format))
    }

    /// Visualize just the VM Plan for an expression
    pub fn visualize_plan(
        &self,
        expr: &str,
        format: crate::visualize::VisualizationFormat,
    ) -> Result<String> {
        use crate::visualize::Visualize;
        let plan = self.compile(expr, None)?;
        Ok(plan.visualize(format))
    }

    /// Code generation: HIR → VM Plan
    fn codegen(&self, hir: crate::hir::HirNode) -> Result<Plan> {
        let mut codegen = CodeGenerator::new();
        codegen.generate(hir)?;
        Ok(codegen.build())
    }
}

/// Result of visualizing the entire compilation pipeline
#[derive(Debug, Clone)]
pub struct PipelineVisualization {
    /// AST visualization
    pub ast: String,
    /// HIR visualization
    pub hir: String,
    /// VM Plan visualization
    pub plan: String,
}
