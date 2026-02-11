use crate::error::{Error, Result};
use crate::hir::{FunctionId, HirNode, PathSegmentHir};
use crate::value::Value;
use crate::vm::{Opcode, Plan};
use std::collections::HashMap;
use std::sync::Arc;

/// Code generator for converting HIR to VM Plan
pub struct CodeGenerator {
    opcodes: Vec<Opcode>,
    constants: Vec<Value>,
    segments: Vec<Arc<str>>,
    type_specifiers: Vec<String>,
    functions: Vec<FunctionId>,
    subplans: Vec<Plan>,
    segment_map: HashMap<Arc<str>, u16>,
    type_spec_map: HashMap<String, u16>,
    variables: Vec<Option<Arc<str>>>,
}

impl Default for CodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            opcodes: Vec::new(),
            constants: Vec::new(),
            segments: Vec::new(),
            type_specifiers: Vec::new(),
            functions: Vec::new(),
            subplans: Vec::new(),
            segment_map: HashMap::new(),
            type_spec_map: HashMap::new(),
            variables: Vec::new(),
        }
    }

    pub fn generate(&mut self, hir: HirNode) -> Result<()> {
        self.generate_node(hir)?;
        self.opcodes.push(Opcode::Return);
        Ok(())
    }

    fn generate_node(&mut self, hir: HirNode) -> Result<()> {
        use crate::hir::HirNode::*;

        match hir {
            Literal { value, .. } => {
                let idx = self.add_constant(value);
                self.opcodes.push(Opcode::PushConst(idx));
            }

            Variable { var_id, name, .. } => match var_id {
                0 => self.opcodes.push(Opcode::LoadThis),
                1 => self.opcodes.push(Opcode::LoadIndex),
                2 => self.opcodes.push(Opcode::LoadTotal),
                _ => {
                    if let Some(name) = name {
                        self.add_variable(var_id, name);
                    }
                    self.opcodes.push(Opcode::PushVariable(var_id));
                }
            },

            Path { base, segments, .. } => {
                // Generate base
                self.generate_node(*base)?;

                // Generate navigation for each segment
                for segment in segments {
                    match segment {
                        PathSegmentHir::Field(field) => {
                            let idx = self.add_segment(Arc::from(field));
                            self.opcodes.push(Opcode::Navigate(idx));
                        }
                        PathSegmentHir::Index(idx) => {
                            self.opcodes.push(Opcode::Index(idx as u16));
                        }
                        PathSegmentHir::Choice(choice) => {
                            // Choice types are handled like fields
                            let idx = self.add_segment(Arc::from(choice));
                            self.opcodes.push(Opcode::Navigate(idx));
                        }
                    }
                }
                // After path navigation, the collection is on the stack
                // If this path ends with a function call, it will be handled separately
            }

            FunctionCall { func_id, args, .. } => {
                // Special handling for iif(): compile arguments as lazy subplans
                if func_id == 300 {
                    if args.len() < 2 || args.len() > 3 {
                        return Err(Error::InvalidOperation(
                            "iif() requires 2 or 3 arguments".into(),
                        ));
                    }

                    // Standalone iif() evaluates its arguments in the current focus ($this).
                    self.opcodes.push(Opcode::LoadThis);

                    let predicate_idx = {
                        let mut cg = CodeGenerator::new();
                        cg.generate_node(args[0].clone())?;
                        cg.opcodes.push(Opcode::Return);
                        let plan = cg.build();
                        let idx = self.subplans.len();
                        self.subplans.push(plan);
                        idx
                    };

                    let true_idx = {
                        let mut cg = CodeGenerator::new();
                        cg.generate_node(args[1].clone())?;
                        cg.opcodes.push(Opcode::Return);
                        let plan = cg.build();
                        let idx = self.subplans.len();
                        self.subplans.push(plan);
                        idx
                    };

                    let false_idx = if args.len() == 3 {
                        let mut cg = CodeGenerator::new();
                        cg.generate_node(args[2].clone())?;
                        cg.opcodes.push(Opcode::Return);
                        let plan = cg.build();
                        let idx = self.subplans.len();
                        self.subplans.push(plan);
                        Some(idx)
                    } else {
                        None
                    };

                    self.opcodes
                        .push(Opcode::Iif(predicate_idx, true_idx, false_idx));
                    return Ok(());
                }

                let arg_count = args.len();

                // Standalone function call - use current focus ($this) as implicit input collection,
                // then arguments so CallFunction pops args first, then base.
                self.opcodes.push(Opcode::LoadThis);

                // Generate arguments after base so they sit on top of the stack
                for arg in args {
                    self.generate_node(arg)?;
                }

                // Record function ID
                if !self.functions.contains(&func_id) {
                    self.functions.push(func_id);
                }

                // Call function
                self.opcodes
                    .push(Opcode::CallFunction(func_id, arg_count as u8));
            }

            MethodCall {
                base,
                func_id,
                args,
                ..
            } => {
                // Special handling for iif(): compile as lazy subplans (base ignored)
                if func_id == 300 {
                    if args.len() < 2 || args.len() > 3 {
                        return Err(Error::InvalidOperation(
                            "iif() requires 2 or 3 arguments".into(),
                        ));
                    }

                    // Evaluate base to provide input collection for iif()
                    self.generate_node(*base)?;

                    let predicate_idx = {
                        let mut cg = CodeGenerator::new();
                        cg.generate_node(args[0].clone())?;
                        cg.opcodes.push(Opcode::Return);
                        let plan = cg.build();
                        let idx = self.subplans.len();
                        self.subplans.push(plan);
                        idx
                    };

                    let true_idx = {
                        let mut cg = CodeGenerator::new();
                        cg.generate_node(args[1].clone())?;
                        cg.opcodes.push(Opcode::Return);
                        let plan = cg.build();
                        let idx = self.subplans.len();
                        self.subplans.push(plan);
                        idx
                    };

                    let false_idx = if args.len() == 3 {
                        let mut cg = CodeGenerator::new();
                        cg.generate_node(args[2].clone())?;
                        cg.opcodes.push(Opcode::Return);
                        let plan = cg.build();
                        let idx = self.subplans.len();
                        self.subplans.push(plan);
                        Some(idx)
                    } else {
                        None
                    };

                    self.opcodes
                        .push(Opcode::Iif(predicate_idx, true_idx, false_idx));
                    return Ok(());
                }

                let arg_count = args.len();

                // Generate base first (will be on bottom of stack)
                self.generate_node(*base)?;

                // Generate arguments second (will be on top of base)
                // Stack: [base] [args...]
                // VM CallFunction pops args first (from top), then collection (base)
                // So we need: [base] [args...] with args on top
                for arg in args {
                    self.generate_node(arg)?;
                }

                // Record function ID
                if !self.functions.contains(&func_id) {
                    self.functions.push(func_id);
                }

                // Call function - VM expects: pop args first, then collection
                // Stack: [base] [args...] → pop args → pop base → correct!
                self.opcodes
                    .push(Opcode::CallFunction(func_id, arg_count as u8));
            }

            BinaryOp {
                op, left, right, ..
            } => {
                // Generate left operand first
                self.generate_node(*left)?;
                // Generate right operand second (will be on top of stack)
                self.generate_node(*right)?;

                // Map HIR operators to implementation IDs (encode operator type)
                let binary_impl_id = match op {
                    crate::hir::HirBinaryOperator::Add => 0,
                    crate::hir::HirBinaryOperator::Sub => 1,
                    crate::hir::HirBinaryOperator::Mul => 2,
                    crate::hir::HirBinaryOperator::Div => 3,
                    crate::hir::HirBinaryOperator::DivInt => 5,
                    crate::hir::HirBinaryOperator::Mod => 4,
                    crate::hir::HirBinaryOperator::Eq => 10,
                    crate::hir::HirBinaryOperator::Ne => 11,
                    crate::hir::HirBinaryOperator::Equivalent => 12,
                    crate::hir::HirBinaryOperator::NotEquivalent => 13,
                    crate::hir::HirBinaryOperator::Lt => 20,
                    crate::hir::HirBinaryOperator::Le => 21,
                    crate::hir::HirBinaryOperator::Gt => 22,
                    crate::hir::HirBinaryOperator::Ge => 23,
                    crate::hir::HirBinaryOperator::And => 30,
                    crate::hir::HirBinaryOperator::Or => 31,
                    crate::hir::HirBinaryOperator::Xor => 32,
                    crate::hir::HirBinaryOperator::Implies => 33,
                    crate::hir::HirBinaryOperator::Union => 40,
                    crate::hir::HirBinaryOperator::In => 41,
                    crate::hir::HirBinaryOperator::Contains => 42,
                    crate::hir::HirBinaryOperator::Concat => 50,
                };

                self.opcodes.push(Opcode::CallBinary(binary_impl_id));
            }

            UnaryOp { op, expr, .. } => {
                // Generate operand
                self.generate_node(*expr)?;

                // Generate unary operator opcode
                match op {
                    crate::hir::HirUnaryOperator::Plus => {
                        self.opcodes.push(Opcode::CallUnary(0)); // Unary plus
                    }
                    crate::hir::HirUnaryOperator::Minus => {
                        self.opcodes.push(Opcode::CallUnary(1)); // Unary minus
                    }
                }
            }

            TypeOp {
                op,
                expr,
                type_specifier,
                ..
            } => {
                // Generate expression
                self.generate_node(*expr)?;

                // Add type specifier to pool
                let type_spec_str = type_specifier.to_string();
                let type_idx = self.add_type_specifier(type_spec_str);

                // Generate type operation opcode
                match op {
                    crate::hir::HirTypeOperator::Is => {
                        self.opcodes.push(Opcode::TypeIs(type_idx));
                    }
                    crate::hir::HirTypeOperator::As => {
                        self.opcodes.push(Opcode::TypeAs(type_idx));
                    }
                }
            }

            Where {
                collection,
                predicate_hir,
                ..
            } => {
                // Generate collection (will be on stack)
                self.generate_node(*collection)?;

                // Compile predicate as subplan
                let mut predicate_codegen = CodeGenerator::new();
                predicate_codegen.generate_node(*predicate_hir)?;
                predicate_codegen.opcodes.push(Opcode::Return);

                let predicate_plan = predicate_codegen.build();

                // Add subplan to plan
                let subplan_idx = self.subplans.len();
                self.subplans.push(predicate_plan);

                // Generate Where opcode with subplan index
                self.opcodes.push(Opcode::Where(subplan_idx));
            }

            Select {
                collection,
                projection_hir,
                ..
            } => {
                // Generate collection (will be on stack)
                self.generate_node(*collection)?;

                // Compile projection as subplan
                let mut projection_codegen = CodeGenerator::new();
                projection_codegen.generate_node(*projection_hir)?;
                projection_codegen.opcodes.push(Opcode::Return);

                let projection_plan = projection_codegen.build();

                // Add subplan to plan
                let subplan_idx = self.subplans.len();
                self.subplans.push(projection_plan);

                // Generate Select opcode with subplan index
                self.opcodes.push(Opcode::Select(subplan_idx));
            }

            Repeat {
                collection,
                projection_hir,
                ..
            } => {
                // Generate collection (will be on stack)
                self.generate_node(*collection)?;

                // Compile projection as subplan
                let mut projection_codegen = CodeGenerator::new();
                projection_codegen.generate_node(*projection_hir)?;
                projection_codegen.opcodes.push(Opcode::Return);

                let projection_plan = projection_codegen.build();

                // Add subplan to plan
                let subplan_idx = self.subplans.len();
                self.subplans.push(projection_plan);

                // Generate Repeat opcode with subplan index
                self.opcodes.push(Opcode::Repeat(subplan_idx));
            }

            Aggregate {
                collection,
                aggregator_hir,
                init_value_hir,
                ..
            } => {
                // Generate collection (will be on stack)
                self.generate_node(*collection)?;

                // Compile aggregator as subplan
                let mut aggregator_codegen = CodeGenerator::new();
                aggregator_codegen.generate_node(*aggregator_hir)?;
                aggregator_codegen.opcodes.push(Opcode::Return);

                let aggregator_plan = aggregator_codegen.build();

                // Add aggregator subplan to plan
                let aggregator_subplan_idx = self.subplans.len();
                self.subplans.push(aggregator_plan);

                // Compile optional init_value as subplan
                let init_value_subplan_idx = if let Some(init_value_hir) = init_value_hir {
                    let mut init_codegen = CodeGenerator::new();
                    init_codegen.generate_node(*init_value_hir)?;
                    init_codegen.opcodes.push(Opcode::Return);

                    let init_plan = init_codegen.build();
                    let idx = self.subplans.len();
                    self.subplans.push(init_plan);
                    Some(idx)
                } else {
                    None
                };

                // Generate Aggregate opcode with subplan indices
                self.opcodes.push(Opcode::Aggregate(
                    aggregator_subplan_idx,
                    init_value_subplan_idx,
                ));
            }

            Exists {
                collection,
                predicate_hir,
                ..
            } => {
                // Generate collection (will be on stack)
                self.generate_node(*collection)?;

                let subplan_idx = if let Some(pred) = predicate_hir {
                    let mut pred_codegen = CodeGenerator::new();
                    pred_codegen.generate_node(*pred)?;
                    pred_codegen.opcodes.push(Opcode::Return);

                    let pred_plan = pred_codegen.build();
                    let idx = self.subplans.len();
                    self.subplans.push(pred_plan);
                    Some(idx)
                } else {
                    None
                };

                self.opcodes.push(Opcode::Exists(subplan_idx));
            }

            All {
                collection,
                predicate_hir,
                ..
            } => {
                // Generate collection (will be on stack)
                self.generate_node(*collection)?;

                // Compile predicate as subplan
                let mut pred_codegen = CodeGenerator::new();
                pred_codegen.generate_node(*predicate_hir)?;
                pred_codegen.opcodes.push(Opcode::Return);

                let pred_plan = pred_codegen.build();
                let idx = self.subplans.len();
                self.subplans.push(pred_plan);

                self.opcodes.push(Opcode::All(idx));
            }
        }

        Ok(())
    }

    fn add_constant(&mut self, value: Value) -> u16 {
        // Check if constant already exists by comparing with existing ones
        for (i, existing) in self.constants.iter().enumerate() {
            if existing == &value {
                return i as u16;
            }
        }

        let idx = self.constants.len() as u16;
        self.constants.push(value);
        idx
    }

    fn add_segment(&mut self, segment: Arc<str>) -> u16 {
        if let Some(&idx) = self.segment_map.get(&segment) {
            return idx;
        }

        let idx = self.segments.len() as u16;
        self.segments.push(segment.clone());
        self.segment_map.insert(segment, idx);
        idx
    }

    fn add_type_specifier(&mut self, type_spec: String) -> u16 {
        if let Some(&idx) = self.type_spec_map.get(&type_spec) {
            return idx;
        }

        let idx = self.type_specifiers.len() as u16;
        self.type_specifiers.push(type_spec.clone());
        self.type_spec_map.insert(type_spec, idx);
        idx
    }

    fn add_variable(&mut self, var_id: u16, name: Arc<str>) {
        let idx = var_id as usize;
        if self.variables.len() <= idx {
            self.variables.resize(idx + 1, None);
        }
        if self.variables[idx].is_none() {
            self.variables[idx] = Some(name);
        }
    }

    pub fn build(self) -> Plan {
        let max_stack_depth = compute_max_stack_depth(&self.opcodes);
        Plan {
            opcodes: self.opcodes,
            max_stack_depth,
            constants: self.constants,
            segments: self.segments,
            type_specifiers: self.type_specifiers,
            functions: self.functions,
            subplans: self.subplans,
            variables: self.variables,
        }
    }
}

fn compute_max_stack_depth(opcodes: &[Opcode]) -> u16 {
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;

    for op in opcodes {
        match *op {
            Opcode::PushConst(_)
            | Opcode::PushVariable(_)
            | Opcode::LoadThis
            | Opcode::LoadIndex
            | Opcode::LoadTotal => {
                depth += 1;
            }
            Opcode::Pop => {
                depth = depth.saturating_sub(1);
            }
            Opcode::Dup => {
                depth += 1;
            }
            Opcode::CallBinary(_) => {
                depth = depth.saturating_sub(1);
            }
            Opcode::CallFunction(_, argc) => {
                depth = depth.saturating_sub(argc as usize);
            }
            Opcode::Return => {
                depth = depth.saturating_sub(1);
            }
            Opcode::Navigate(_)
            | Opcode::Index(_)
            | Opcode::CallUnary(_)
            | Opcode::TypeIs(_)
            | Opcode::TypeAs(_)
            | Opcode::Where(_)
            | Opcode::Select(_)
            | Opcode::Repeat(_)
            | Opcode::Aggregate(_, _)
            | Opcode::Exists(_)
            | Opcode::All(_)
            | Opcode::Jump(_)
            | Opcode::JumpIfEmpty(_)
            | Opcode::JumpIfNotEmpty(_)
            | Opcode::Iif(_, _, _) => {}
        }
        max_depth = max_depth.max(depth);
    }

    max_depth.min(u16::MAX as usize) as u16
}
