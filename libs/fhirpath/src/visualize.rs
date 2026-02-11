//! Visualization utilities for compiler intermediate representations
//!
//! This module provides visualization capabilities for:
//! - AST (Abstract Syntax Tree)
//! - HIR (High-level Intermediate Representation)
//! - VM Plans (bytecode)
//!
//! Supports multiple output formats:
//! - Mermaid diagrams (for markdown/web rendering)
//! - DOT/Graphviz (for generating PNG/SVG)
//! - ASCII tree (for terminal viewing)

use crate::ast::AstNode;
use crate::hir::HirNode;
use crate::types::{ExprType, TypeNamespace};
use crate::vm::{Opcode, Plan};
use std::fmt::Write as FmtWrite;

/// Visualization format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizationFormat {
    /// Mermaid diagram format (renders in markdown, GitHub, etc.)
    Mermaid,
    /// DOT/Graphviz format (can be rendered with `dot` command)
    Dot,
    /// ASCII tree format (for terminal viewing)
    AsciiTree,
}

/// Trait for types that can be visualized
pub trait Visualize {
    /// Generate visualization in the specified format
    fn visualize(&self, format: VisualizationFormat) -> String;
}

// =============================================================================
// AST Visualization
// =============================================================================

impl Visualize for AstNode {
    fn visualize(&self, format: VisualizationFormat) -> String {
        match format {
            VisualizationFormat::Mermaid => visualize_ast_mermaid(self),
            VisualizationFormat::Dot => visualize_ast_dot(self),
            VisualizationFormat::AsciiTree => visualize_ast_ascii(self, 0),
        }
    }
}

fn visualize_ast_mermaid(node: &AstNode) -> String {
    let mut output = String::from("graph TD\n");
    let mut counter = 0;
    visit_ast_mermaid(node, &mut counter, None, &mut output);
    output
}

fn visit_ast_mermaid(
    node: &AstNode,
    counter: &mut usize,
    parent_id: Option<usize>,
    output: &mut String,
) {
    let current_id = *counter;
    *counter += 1;

    let label = ast_node_label(node);
    let _ = writeln!(output, "    n{}[\"{}\"]", current_id, label);

    if let Some(parent) = parent_id {
        let _ = writeln!(output, "    n{} --> n{}", parent, current_id);
    }

    // Visit children
    match node {
        AstNode::TermExpression { term }
        | AstNode::InvocationTerm { invocation: term }
        | AstNode::LiteralTerm { literal: term }
        | AstNode::ParenthesizedTerm { expression: term } => {
            visit_ast_mermaid(term, counter, Some(current_id), output);
        }
        AstNode::InvocationExpression {
            expression,
            invocation,
        } => {
            visit_ast_mermaid(expression, counter, Some(current_id), output);
            visit_ast_mermaid(invocation, counter, Some(current_id), output);
        }
        AstNode::IndexerExpression { collection, index } => {
            visit_ast_mermaid(collection, counter, Some(current_id), output);
            visit_ast_mermaid(index, counter, Some(current_id), output);
        }
        AstNode::PolarityExpression {
            operator: _,
            expression,
        } => {
            visit_ast_mermaid(expression, counter, Some(current_id), output);
        }
        AstNode::MultiplicativeExpression { left, right, .. }
        | AstNode::AdditiveExpression { left, right, .. }
        | AstNode::UnionExpression { left, right }
        | AstNode::InequalityExpression { left, right, .. }
        | AstNode::EqualityExpression { left, right, .. }
        | AstNode::MembershipExpression { left, right, .. }
        | AstNode::AndExpression { left, right }
        | AstNode::OrExpression { left, right, .. }
        | AstNode::ImpliesExpression { left, right } => {
            visit_ast_mermaid(left, counter, Some(current_id), output);
            visit_ast_mermaid(right, counter, Some(current_id), output);
        }
        AstNode::TypeExpression {
            expression,
            type_specifier,
            ..
        } => {
            visit_ast_mermaid(expression, counter, Some(current_id), output);
            let type_id = *counter;
            *counter += 1;
            let _ = writeln!(output, "    n{}[\"Type: {:?}\"]", type_id, type_specifier);
            let _ = writeln!(output, "    n{} --> n{}", current_id, type_id);
        }
        AstNode::FunctionInvocation {
            function_name: _,
            parameters,
        } => {
            for param in parameters {
                visit_ast_mermaid(param, counter, Some(current_id), output);
            }
        }
        AstNode::CollectionLiteral { elements } => {
            for elem in elements {
                visit_ast_mermaid(elem, counter, Some(current_id), output);
            }
        }
        _ => {} // Leaf nodes
    }
}

fn visualize_ast_dot(node: &AstNode) -> String {
    let mut output = String::from("digraph AST {\n");
    output.push_str("    node [shape=box, style=rounded];\n");
    let mut counter = 0;
    visit_ast_dot(node, &mut counter, None, &mut output);
    output.push_str("}\n");
    output
}

fn visit_ast_dot(
    node: &AstNode,
    counter: &mut usize,
    parent_id: Option<usize>,
    output: &mut String,
) {
    let current_id = *counter;
    *counter += 1;

    let label = ast_node_label(node);
    let _ = writeln!(output, "    n{} [label=\"{}\"];", current_id, label);

    if let Some(parent) = parent_id {
        let _ = writeln!(output, "    n{} -> n{};", parent, current_id);
    }

    // Visit children (same logic as mermaid)
    match node {
        AstNode::TermExpression { term }
        | AstNode::InvocationTerm { invocation: term }
        | AstNode::LiteralTerm { literal: term }
        | AstNode::ParenthesizedTerm { expression: term } => {
            visit_ast_dot(term, counter, Some(current_id), output);
        }
        AstNode::InvocationExpression {
            expression,
            invocation,
        } => {
            visit_ast_dot(expression, counter, Some(current_id), output);
            visit_ast_dot(invocation, counter, Some(current_id), output);
        }
        AstNode::IndexerExpression { collection, index } => {
            visit_ast_dot(collection, counter, Some(current_id), output);
            visit_ast_dot(index, counter, Some(current_id), output);
        }
        AstNode::PolarityExpression {
            operator: _,
            expression,
        } => {
            visit_ast_dot(expression, counter, Some(current_id), output);
        }
        AstNode::MultiplicativeExpression { left, right, .. }
        | AstNode::AdditiveExpression { left, right, .. }
        | AstNode::UnionExpression { left, right }
        | AstNode::InequalityExpression { left, right, .. }
        | AstNode::EqualityExpression { left, right, .. }
        | AstNode::MembershipExpression { left, right, .. }
        | AstNode::AndExpression { left, right }
        | AstNode::OrExpression { left, right, .. }
        | AstNode::ImpliesExpression { left, right } => {
            visit_ast_dot(left, counter, Some(current_id), output);
            visit_ast_dot(right, counter, Some(current_id), output);
        }
        AstNode::TypeExpression {
            expression,
            type_specifier,
            ..
        } => {
            visit_ast_dot(expression, counter, Some(current_id), output);
            let type_id = *counter;
            *counter += 1;
            let _ = writeln!(
                output,
                "    n{} [label=\"Type: {:?}\"];",
                type_id, type_specifier
            );
            let _ = writeln!(output, "    n{} -> n{};", current_id, type_id);
        }
        AstNode::FunctionInvocation {
            function_name: _,
            parameters,
        } => {
            for param in parameters {
                visit_ast_dot(param, counter, Some(current_id), output);
            }
        }
        AstNode::CollectionLiteral { elements } => {
            for elem in elements {
                visit_ast_dot(elem, counter, Some(current_id), output);
            }
        }
        _ => {} // Leaf nodes
    }
}

fn visualize_ast_ascii(node: &AstNode, depth: usize) -> String {
    let indent = "  ".repeat(depth);
    let mut output = format!("{}├─ {}\n", indent, ast_node_label(node));

    // Visit children
    match node {
        AstNode::TermExpression { term }
        | AstNode::InvocationTerm { invocation: term }
        | AstNode::LiteralTerm { literal: term }
        | AstNode::ParenthesizedTerm { expression: term } => {
            output.push_str(&visualize_ast_ascii(term, depth + 1));
        }
        AstNode::InvocationExpression {
            expression,
            invocation,
        } => {
            output.push_str(&visualize_ast_ascii(expression, depth + 1));
            output.push_str(&visualize_ast_ascii(invocation, depth + 1));
        }
        AstNode::IndexerExpression { collection, index } => {
            output.push_str(&visualize_ast_ascii(collection, depth + 1));
            output.push_str(&visualize_ast_ascii(index, depth + 1));
        }
        AstNode::PolarityExpression {
            operator: _,
            expression,
        } => {
            output.push_str(&visualize_ast_ascii(expression, depth + 1));
        }
        AstNode::MultiplicativeExpression { left, right, .. }
        | AstNode::AdditiveExpression { left, right, .. }
        | AstNode::UnionExpression { left, right }
        | AstNode::InequalityExpression { left, right, .. }
        | AstNode::EqualityExpression { left, right, .. }
        | AstNode::MembershipExpression { left, right, .. }
        | AstNode::AndExpression { left, right }
        | AstNode::OrExpression { left, right, .. }
        | AstNode::ImpliesExpression { left, right } => {
            output.push_str(&visualize_ast_ascii(left, depth + 1));
            output.push_str(&visualize_ast_ascii(right, depth + 1));
        }
        AstNode::TypeExpression {
            expression,
            type_specifier,
            ..
        } => {
            output.push_str(&visualize_ast_ascii(expression, depth + 1));
            output.push_str(&format!("{}  ├─ Type: {:?}\n", indent, type_specifier));
        }
        AstNode::FunctionInvocation {
            function_name: _,
            parameters,
        } => {
            for param in parameters {
                output.push_str(&visualize_ast_ascii(param, depth + 1));
            }
        }
        AstNode::CollectionLiteral { elements } => {
            for elem in elements {
                output.push_str(&visualize_ast_ascii(elem, depth + 1));
            }
        }
        _ => {} // Leaf nodes
    }

    output
}

fn ast_node_label(node: &AstNode) -> String {
    match node {
        AstNode::NullLiteral => "null".to_string(),
        AstNode::BooleanLiteral(b) => format!("Boolean: {}", b),
        AstNode::StringLiteral(s) => format!("String: \"{}\"", s),
        AstNode::IntegerLiteral(i) => format!("Integer: {}", i),
        AstNode::NumberLiteral(d) => format!("Decimal: {}", d),
        AstNode::LongNumberLiteral(i) => format!("Long: {}", i),
        AstNode::DateLiteral(d, _) => format!("Date: {}", d),
        AstNode::DateTimeLiteral(dt, _, _) => format!("DateTime: {}", dt),
        AstNode::TimeLiteral(t, _) => format!("Time: {}", t),
        AstNode::QuantityLiteral { value, unit } => {
            format!("Quantity: {} {:?}", value, unit)
        }
        AstNode::CollectionLiteral { elements } => format!("Collection[{}]", elements.len()),
        AstNode::TermExpression { .. } => "Term".to_string(),
        AstNode::InvocationTerm { .. } => "Invocation".to_string(),
        AstNode::LiteralTerm { .. } => "Literal".to_string(),
        AstNode::ParenthesizedTerm { .. } => "()".to_string(),
        AstNode::ExternalConstantTerm { constant } => format!("External: {}", constant),
        AstNode::MemberInvocation { identifier } => format!("Field: {}", identifier),
        AstNode::FunctionInvocation { function_name, .. } => format!("Fn: {}()", function_name),
        AstNode::ThisInvocation => "$this".to_string(),
        AstNode::IndexInvocation => "$index".to_string(),
        AstNode::TotalInvocation => "$total".to_string(),
        AstNode::InvocationExpression { .. } => "Path".to_string(),
        AstNode::IndexerExpression { .. } => "[]".to_string(),
        AstNode::PolarityExpression { operator, .. } => format!("Polarity: {:?}", operator),
        AstNode::MultiplicativeExpression { operator, .. } => format!("Mul: {:?}", operator),
        AstNode::AdditiveExpression { operator, .. } => format!("Add: {:?}", operator),
        AstNode::UnionExpression { .. } => "|".to_string(),
        AstNode::InequalityExpression { operator, .. } => format!("Cmp: {:?}", operator),
        AstNode::EqualityExpression { operator, .. } => format!("Eq: {:?}", operator),
        AstNode::MembershipExpression { operator, .. } => format!("Member: {:?}", operator),
        AstNode::AndExpression { .. } => "and".to_string(),
        AstNode::OrExpression { operator, .. } => format!("{:?}", operator),
        AstNode::ImpliesExpression { .. } => "implies".to_string(),
        AstNode::TypeExpression { operator, .. } => format!("Type: {:?}", operator),
    }
}

// =============================================================================
// HIR Visualization
// =============================================================================

impl Visualize for HirNode {
    fn visualize(&self, format: VisualizationFormat) -> String {
        match format {
            VisualizationFormat::Mermaid => visualize_hir_mermaid(self),
            VisualizationFormat::Dot => visualize_hir_dot(self),
            VisualizationFormat::AsciiTree => visualize_hir_ascii(self, 0),
        }
    }
}

fn format_expr_type(ty: &ExprType) -> String {
    let types = if ty.types.is_unknown() {
        "Any".to_string()
    } else {
        ty.types
            .iter()
            .map(|t| match t.namespace {
                TypeNamespace::System => format!("System.{}", t.name),
                TypeNamespace::Fhir => format!("FHIR.{}", t.name),
            })
            .collect::<Vec<_>>()
            .join("|")
    };

    let card = match ty.cardinality.max {
        Some(max) => format!("{}..{}", ty.cardinality.min, max),
        None => format!("{}..*", ty.cardinality.min),
    };

    format!("[{}] {}", types, card)
}

fn format_expr_type_compact(ty: &ExprType) -> String {
    let types = if ty.types.is_unknown() {
        "Any".to_string()
    } else {
        ty.types
            .iter()
            .map(|t| match t.namespace {
                TypeNamespace::System => t.name.as_ref().to_string(),
                TypeNamespace::Fhir => format!("{}", t.name),
            })
            .collect::<Vec<_>>()
            .join("|")
    };

    let card = match ty.cardinality.max {
        Some(max) if ty.cardinality.min == max && max == 1 => "".to_string(),
        Some(max) => format!(" [{}..{}]", ty.cardinality.min, max),
        None => format!(" [{}..*]", ty.cardinality.min),
    };

    format!("{}{}", types, card)
}

fn visualize_hir_mermaid(node: &HirNode) -> String {
    let mut output = String::from("graph TD\n");
    let mut counter = 0;
    visit_hir_mermaid(node, &mut counter, None, &mut output);
    output
}

fn visit_hir_mermaid(
    node: &HirNode,
    counter: &mut usize,
    parent_id: Option<usize>,
    output: &mut String,
) {
    let current_id = *counter;
    *counter += 1;

    let label = hir_node_label_with_types(node);
    let _ = writeln!(output, "    n{}[\"{}\"]", current_id, label);

    if let Some(parent) = parent_id {
        let _ = writeln!(output, "    n{} --> n{}", parent, current_id);
    }

    // Visit children
    match node {
        HirNode::Path { base, .. } => {
            visit_hir_mermaid(base, counter, Some(current_id), output);
        }
        HirNode::BinaryOp { left, right, .. } => {
            visit_hir_mermaid(left, counter, Some(current_id), output);
            visit_hir_mermaid(right, counter, Some(current_id), output);
        }
        HirNode::UnaryOp { expr, .. } => {
            visit_hir_mermaid(expr, counter, Some(current_id), output);
        }
        HirNode::FunctionCall { args, .. } => {
            for arg in args {
                visit_hir_mermaid(arg, counter, Some(current_id), output);
            }
        }
        HirNode::MethodCall { base, args, .. } => {
            visit_hir_mermaid(base, counter, Some(current_id), output);
            for arg in args {
                visit_hir_mermaid(arg, counter, Some(current_id), output);
            }
        }
        HirNode::Where {
            collection,
            predicate_hir,
            ..
        } => {
            visit_hir_mermaid(collection, counter, Some(current_id), output);
            visit_hir_mermaid(predicate_hir, counter, Some(current_id), output);
        }
        HirNode::Select {
            collection,
            projection_hir,
            ..
        } => {
            visit_hir_mermaid(collection, counter, Some(current_id), output);
            visit_hir_mermaid(projection_hir, counter, Some(current_id), output);
        }
        HirNode::Repeat {
            collection,
            projection_hir,
            ..
        } => {
            visit_hir_mermaid(collection, counter, Some(current_id), output);
            visit_hir_mermaid(projection_hir, counter, Some(current_id), output);
        }
        HirNode::Aggregate {
            collection,
            aggregator_hir,
            init_value_hir,
            ..
        } => {
            visit_hir_mermaid(collection, counter, Some(current_id), output);
            visit_hir_mermaid(aggregator_hir, counter, Some(current_id), output);
            if let Some(init) = init_value_hir {
                visit_hir_mermaid(init, counter, Some(current_id), output);
            }
        }
        HirNode::Exists {
            collection,
            predicate_hir,
            ..
        } => {
            visit_hir_mermaid(collection, counter, Some(current_id), output);
            if let Some(pred) = predicate_hir {
                visit_hir_mermaid(pred, counter, Some(current_id), output);
            }
        }
        HirNode::TypeOp { expr, .. } => {
            visit_hir_mermaid(expr, counter, Some(current_id), output);
        }
        _ => {} // Leaf nodes
    }
}

fn visualize_hir_dot(node: &HirNode) -> String {
    let mut output = String::from("digraph HIR {\n");
    output.push_str("    node [shape=box, style=\"rounded,filled\", fillcolor=lightblue];\n");
    let mut counter = 0;
    visit_hir_dot(node, &mut counter, None, &mut output);
    output.push_str("}\n");
    output
}

fn visit_hir_dot(
    node: &HirNode,
    counter: &mut usize,
    parent_id: Option<usize>,
    output: &mut String,
) {
    let current_id = *counter;
    *counter += 1;

    let label = hir_node_label_with_types(node);
    // Escape special characters for DOT format
    let escaped_label = label.replace('"', "\\\"").replace('\n', "\\n");
    let _ = writeln!(output, "    n{} [label=\"{}\"];", current_id, escaped_label);

    if let Some(parent) = parent_id {
        let _ = writeln!(output, "    n{} -> n{};", parent, current_id);
    }

    // Visit children (same as mermaid)
    match node {
        HirNode::Path { base, .. } => {
            visit_hir_dot(base, counter, Some(current_id), output);
        }
        HirNode::BinaryOp { left, right, .. } => {
            visit_hir_dot(left, counter, Some(current_id), output);
            visit_hir_dot(right, counter, Some(current_id), output);
        }
        HirNode::UnaryOp { expr, .. } => {
            visit_hir_dot(expr, counter, Some(current_id), output);
        }
        HirNode::FunctionCall { args, .. } => {
            for arg in args {
                visit_hir_dot(arg, counter, Some(current_id), output);
            }
        }
        HirNode::MethodCall { base, args, .. } => {
            visit_hir_dot(base, counter, Some(current_id), output);
            for arg in args {
                visit_hir_dot(arg, counter, Some(current_id), output);
            }
        }
        HirNode::Where {
            collection,
            predicate_hir,
            ..
        } => {
            visit_hir_dot(collection, counter, Some(current_id), output);
            visit_hir_dot(predicate_hir, counter, Some(current_id), output);
        }
        HirNode::Select {
            collection,
            projection_hir,
            ..
        } => {
            visit_hir_dot(collection, counter, Some(current_id), output);
            visit_hir_dot(projection_hir, counter, Some(current_id), output);
        }
        HirNode::Repeat {
            collection,
            projection_hir,
            ..
        } => {
            visit_hir_dot(collection, counter, Some(current_id), output);
            visit_hir_dot(projection_hir, counter, Some(current_id), output);
        }
        HirNode::Aggregate {
            collection,
            aggregator_hir,
            init_value_hir,
            ..
        } => {
            visit_hir_dot(collection, counter, Some(current_id), output);
            visit_hir_dot(aggregator_hir, counter, Some(current_id), output);
            if let Some(init) = init_value_hir {
                visit_hir_dot(init, counter, Some(current_id), output);
            }
        }
        HirNode::Exists {
            collection,
            predicate_hir,
            ..
        } => {
            visit_hir_dot(collection, counter, Some(current_id), output);
            if let Some(pred) = predicate_hir {
                visit_hir_dot(pred, counter, Some(current_id), output);
            }
        }
        HirNode::TypeOp { expr, .. } => {
            visit_hir_dot(expr, counter, Some(current_id), output);
        }
        _ => {} // Leaf nodes
    }
}

fn visualize_hir_ascii(node: &HirNode, depth: usize) -> String {
    let indent = "  ".repeat(depth);
    let label = hir_node_label_with_types(node);
    let mut output = format!("{}├─ {}\n", indent, label);

    // Visit children
    match node {
        HirNode::Path { base, segments, .. } => {
            output.push_str(&visualize_hir_ascii(base, depth + 1));
            for seg in segments {
                output.push_str(&format!("{}  ├─ .{:?}\n", indent, seg));
            }
        }
        HirNode::BinaryOp { left, right, .. } => {
            output.push_str(&visualize_hir_ascii(left, depth + 1));
            output.push_str(&visualize_hir_ascii(right, depth + 1));
        }
        HirNode::UnaryOp { expr, .. } => {
            output.push_str(&visualize_hir_ascii(expr, depth + 1));
        }
        HirNode::FunctionCall { args, .. } => {
            for arg in args {
                output.push_str(&visualize_hir_ascii(arg, depth + 1));
            }
        }
        HirNode::MethodCall { base, args, .. } => {
            output.push_str(&visualize_hir_ascii(base, depth + 1));
            for arg in args {
                output.push_str(&visualize_hir_ascii(arg, depth + 1));
            }
        }
        HirNode::Where {
            collection,
            predicate_hir,
            ..
        } => {
            output.push_str(&visualize_hir_ascii(collection, depth + 1));
            output.push_str(&format!("{}  ├─ [predicate]\n", indent));
            output.push_str(&visualize_hir_ascii(predicate_hir, depth + 2));
        }
        HirNode::Select {
            collection,
            projection_hir,
            ..
        } => {
            output.push_str(&visualize_hir_ascii(collection, depth + 1));
            output.push_str(&format!("{}  ├─ [projection]\n", indent));
            output.push_str(&visualize_hir_ascii(projection_hir, depth + 2));
        }
        HirNode::Repeat {
            collection,
            projection_hir,
            ..
        } => {
            output.push_str(&visualize_hir_ascii(collection, depth + 1));
            output.push_str(&format!("{}  ├─ [repeat]\n", indent));
            output.push_str(&visualize_hir_ascii(projection_hir, depth + 2));
        }
        HirNode::Aggregate {
            collection,
            aggregator_hir,
            init_value_hir,
            ..
        } => {
            output.push_str(&visualize_hir_ascii(collection, depth + 1));
            output.push_str(&format!("{}  ├─ [aggregator]\n", indent));
            output.push_str(&visualize_hir_ascii(aggregator_hir, depth + 2));
            if let Some(init) = init_value_hir {
                output.push_str(&format!("{}  ├─ [init]\n", indent));
                output.push_str(&visualize_hir_ascii(init, depth + 2));
            }
        }
        HirNode::Exists {
            collection,
            predicate_hir,
            ..
        } => {
            output.push_str(&visualize_hir_ascii(collection, depth + 1));
            if let Some(pred) = predicate_hir {
                output.push_str(&format!("{}  ├─ [predicate]\n", indent));
                output.push_str(&visualize_hir_ascii(pred, depth + 2));
            }
        }
        HirNode::TypeOp { expr, .. } => {
            output.push_str(&visualize_hir_ascii(expr, depth + 1));
        }
        _ => {} // Leaf nodes
    }

    output
}

#[allow(dead_code)]
fn hir_node_label(node: &HirNode) -> String {
    match node {
        HirNode::Literal { value, .. } => format!("Lit: {:?}", value),
        HirNode::Variable { var_id, name, .. } => {
            if let Some(n) = name {
                format!("Var: {}", n)
            } else {
                format!("Var[{}]", var_id)
            }
        }
        HirNode::Path { segments, .. } => {
            let path = segments
                .iter()
                .filter_map(|s| s.as_field())
                .collect::<Vec<_>>()
                .join(".");
            format!("Path: {}", path)
        }
        HirNode::BinaryOp { op, .. } => format!("BinOp: {:?}", op),
        HirNode::UnaryOp { op, .. } => format!("UnaryOp: {:?}", op),
        HirNode::FunctionCall { func_id, args, .. } => format!("Fn[{}]({})", func_id, args.len()),
        HirNode::MethodCall { func_id, args, .. } => format!("Method[{}]({})", func_id, args.len()),
        HirNode::Where { .. } => "where()".to_string(),
        HirNode::Select { .. } => "select()".to_string(),
        HirNode::Repeat { .. } => "repeat()".to_string(),
        HirNode::Aggregate { .. } => "aggregate()".to_string(),
        HirNode::All { .. } => "all()".to_string(),
        HirNode::Exists { .. } => "exists()".to_string(),
        HirNode::TypeOp {
            op, type_specifier, ..
        } => format!("{:?} {}", op, type_specifier),
    }
}

fn hir_node_label_with_types(node: &HirNode) -> String {
    match node {
        HirNode::Literal { value, ty } => {
            format!("Lit: {:?}\nType: {}", value, format_expr_type(ty))
        }
        HirNode::Variable { var_id, name, ty } => {
            let var_name = if let Some(n) = name {
                format!("Var: {}", n)
            } else {
                format!("Var[{}]", var_id)
            };
            format!("{}\nType: {}", var_name, format_expr_type(ty))
        }
        HirNode::Path {
            segments,
            result_ty,
            base,
        } => {
            let path = segments
                .iter()
                .filter_map(|s| s.as_field())
                .collect::<Vec<_>>()
                .join(".");
            let base_type = base
                .result_type()
                .map(|t| format!("Base: {}\n", format_expr_type_compact(&t)))
                .unwrap_or_default();
            format!(
                "Path: {}\n{}Result: {}",
                path,
                base_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::BinaryOp {
            op,
            left,
            right,
            result_ty,
            ..
        } => {
            let left_type = left
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let right_type = right
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            format!(
                "BinOp: {:?}\nLeft: {}\nRight: {}\nResult: {}",
                op,
                left_type,
                right_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::UnaryOp {
            op,
            expr,
            result_ty,
        } => {
            let expr_type = expr
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            format!(
                "UnaryOp: {:?}\nOperand: {}\nResult: {}",
                op,
                expr_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::FunctionCall {
            func_id,
            args,
            result_ty,
        } => {
            let arg_types: Vec<String> = args
                .iter()
                .map(|arg| {
                    arg.result_type()
                        .map(|t| format_expr_type_compact(&t))
                        .unwrap_or_else(|| "?".to_string())
                })
                .collect();
            format!(
                "Fn[{}]({})\nArgs: {}\nResult: {}",
                func_id,
                args.len(),
                arg_types.join(", "),
                format_expr_type(result_ty)
            )
        }
        HirNode::MethodCall {
            base,
            func_id,
            args,
            result_ty,
        } => {
            let base_type = base
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let arg_types: Vec<String> = args
                .iter()
                .map(|arg| {
                    arg.result_type()
                        .map(|t| format_expr_type_compact(&t))
                        .unwrap_or_else(|| "?".to_string())
                })
                .collect();
            format!(
                "Method[{}]({})\nBase: {}\nArgs: {}\nResult: {}",
                func_id,
                args.len(),
                base_type,
                arg_types.join(", "),
                format_expr_type(result_ty)
            )
        }
        HirNode::Where {
            collection,
            predicate_hir,
            result_ty,
            ..
        } => {
            let coll_type = collection
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let pred_type = predicate_hir
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            format!(
                "where()\nCollection: {}\nPredicate: {}\nResult: {}",
                coll_type,
                pred_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::Select {
            collection,
            projection_hir,
            result_ty,
            ..
        } => {
            let coll_type = collection
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let proj_type = projection_hir
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            format!(
                "select()\nCollection: {}\nProjection: {}\nResult: {}",
                coll_type,
                proj_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::Repeat {
            collection,
            projection_hir,
            result_ty,
            ..
        } => {
            let coll_type = collection
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let proj_type = projection_hir
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            format!(
                "repeat()\nCollection: {}\nProjection: {}\nResult: {}",
                coll_type,
                proj_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::Aggregate {
            collection,
            aggregator_hir,
            init_value_hir,
            result_ty,
            ..
        } => {
            let coll_type = collection
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let agg_type = aggregator_hir
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let init_type = init_value_hir
                .as_ref()
                .and_then(|init| init.result_type())
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "None".to_string());
            format!(
                "aggregate()\nCollection: {}\nAggregator: {}\nInit: {}\nResult: {}",
                coll_type,
                agg_type,
                init_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::All {
            collection,
            predicate_hir,
            result_ty,
            ..
        } => {
            let coll_type = collection
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let pred_type = predicate_hir
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            format!(
                "all()\nCollection: {}\nPredicate: {}\nResult: {}",
                coll_type,
                pred_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::Exists {
            collection,
            predicate_hir,
            result_ty,
            ..
        } => {
            let coll_type = collection
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            let pred_type = predicate_hir
                .as_ref()
                .and_then(|pred| pred.result_type())
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "None".to_string());
            format!(
                "exists()\nCollection: {}\nPredicate: {}\nResult: {}",
                coll_type,
                pred_type,
                format_expr_type(result_ty)
            )
        }
        HirNode::TypeOp {
            op,
            expr,
            type_specifier,
            result_ty,
        } => {
            let expr_type = expr
                .result_type()
                .map(|t| format_expr_type_compact(&t))
                .unwrap_or_else(|| "?".to_string());
            format!(
                "{:?} {}\nOperand: {}\nResult: {}",
                op,
                type_specifier,
                expr_type,
                format_expr_type(result_ty)
            )
        }
    }
}

// =============================================================================
// VM Plan Visualization
// =============================================================================

impl Visualize for Plan {
    fn visualize(&self, format: VisualizationFormat) -> String {
        match format {
            VisualizationFormat::Mermaid => visualize_plan_mermaid(self),
            VisualizationFormat::Dot => visualize_plan_dot(self),
            VisualizationFormat::AsciiTree => visualize_plan_ascii(self),
        }
    }
}

fn visualize_plan_mermaid(plan: &Plan) -> String {
    let mut output = String::from("graph LR\n");
    output.push_str("    subgraph \"Main Plan\"\n");

    for (i, opcode) in plan.opcodes.iter().enumerate() {
        let label = format_opcode(opcode, plan);
        let _ = writeln!(output, "        i{}[\"{}. {}\"]", i, i, label);
        if i > 0 {
            let _ = writeln!(output, "        i{} --> i{}", i - 1, i);
        }
    }

    output.push_str("    end\n");

    // Show subplans
    for (id, subplan) in plan.subplans.iter().enumerate() {
        output.push_str(&format!("    subgraph \"Subplan {}\"\n", id));
        for (i, opcode) in subplan.opcodes.iter().enumerate() {
            let label = format_opcode(opcode, subplan);
            let _ = writeln!(output, "        s{}_{}[\"{}. {}\"]", id, i, i, label);
            if i > 0 {
                let _ = writeln!(output, "        s{}_{} --> s{}_{}", id, i - 1, id, i);
            }
        }
        output.push_str("    end\n");
    }

    output
}

fn visualize_plan_dot(plan: &Plan) -> String {
    let mut output = String::from("digraph Plan {\n");
    output.push_str("    rankdir=TB;\n");
    output.push_str("    node [shape=box, style=\"rounded,filled\", fillcolor=lightyellow];\n");

    output.push_str("    subgraph cluster_main {\n");
    output.push_str("        label=\"Main Plan\";\n");
    output.push_str("        style=filled;\n");
    output.push_str("        color=lightgrey;\n");

    for (i, opcode) in plan.opcodes.iter().enumerate() {
        let label = format_opcode(opcode, plan);
        let _ = writeln!(output, "        i{} [label=\"{}. {}\"];", i, i, label);
        if i > 0 {
            let _ = writeln!(output, "        i{} -> i{};", i - 1, i);
        }
    }

    output.push_str("    }\n");

    // Show subplans
    for (id, subplan) in plan.subplans.iter().enumerate() {
        output.push_str(&format!("    subgraph cluster_sub{} {{\n", id));
        output.push_str(&format!("        label=\"Subplan {}\";\n", id));
        output.push_str("        style=filled;\n");
        output.push_str("        color=lightblue;\n");

        for (i, opcode) in subplan.opcodes.iter().enumerate() {
            let label = format_opcode(opcode, subplan);
            let _ = writeln!(
                output,
                "        s{}_i{} [label=\"{}. {}\"];",
                id, i, i, label
            );
            if i > 0 {
                let _ = writeln!(output, "        s{}_i{} -> s{}_i{};", id, i - 1, id, i);
            }
        }

        output.push_str("    }\n");
    }

    output.push_str("}\n");
    output
}

fn visualize_plan_ascii(plan: &Plan) -> String {
    let mut output = String::from("VM Plan\n");
    output.push_str("========\n\n");
    output.push_str("Main Opcodes:\n");

    for (i, opcode) in plan.opcodes.iter().enumerate() {
        let label = format_opcode(opcode, plan);
        output.push_str(&format!("  {:3}: {}\n", i, label));
    }

    if !plan.subplans.is_empty() {
        output.push_str("\nSubplans:\n");
        for (id, subplan) in plan.subplans.iter().enumerate() {
            output.push_str(&format!("\n  Subplan {}:\n", id));
            for (i, opcode) in subplan.opcodes.iter().enumerate() {
                let label = format_opcode(opcode, subplan);
                output.push_str(&format!("    {:3}: {}\n", i, label));
            }
        }
    }

    output
}

fn format_opcode(opcode: &Opcode, plan: &Plan) -> String {
    match opcode {
        Opcode::PushConst(idx) => {
            let value = plan
                .constants
                .get(*idx as usize)
                .map(|v| format!("{:?}", v))
                .unwrap_or_else(|| "?".to_string());
            format!("PUSH_CONST[{}] = {}", idx, value)
        }
        Opcode::PushVariable(id) => format!("PUSH_VAR ${}", id),
        Opcode::LoadThis => "LOAD_THIS".to_string(),
        Opcode::LoadIndex => "LOAD_INDEX".to_string(),
        Opcode::LoadTotal => "LOAD_TOTAL".to_string(),
        Opcode::Pop => "POP".to_string(),
        Opcode::Dup => "DUP".to_string(),
        Opcode::Navigate(idx) => {
            let field = plan
                .segments
                .get(*idx as usize)
                .map(|s| s.as_ref())
                .unwrap_or("?");
            format!("NAVIGATE .{}", field)
        }
        Opcode::Index(idx) => format!("INDEX [{}]", idx),
        Opcode::CallBinary(impl_id) => format!("CALL_BINARY #{}", impl_id),
        Opcode::CallUnary(op) => {
            let op_name = match *op {
                0 => "+",
                1 => "-",
                _ => "?",
            };
            format!("CALL_UNARY {}", op_name)
        }
        Opcode::TypeIs(idx) => {
            let type_spec = plan
                .type_specifiers
                .get(*idx as usize)
                .map(|s| s.as_str())
                .unwrap_or("?");
            format!("TYPE_IS {}", type_spec)
        }
        Opcode::TypeAs(idx) => {
            let type_spec = plan
                .type_specifiers
                .get(*idx as usize)
                .map(|s| s.as_str())
                .unwrap_or("?");
            format!("TYPE_AS {}", type_spec)
        }
        Opcode::CallFunction(func_id, argc) => format!("CALL_FN #{}({})", func_id, argc),
        Opcode::Where(plan_id) => format!("WHERE subplan[{}]", plan_id),
        Opcode::Select(plan_id) => format!("SELECT subplan[{}]", plan_id),
        Opcode::Repeat(plan_id) => format!("REPEAT subplan[{}]", plan_id),
        Opcode::Aggregate(plan_id, init_id) => {
            if let Some(init) = init_id {
                format!("AGGREGATE subplan[{}] init[{}]", plan_id, init)
            } else {
                format!("AGGREGATE subplan[{}]", plan_id)
            }
        }
        Opcode::Exists(pred_id) => {
            if let Some(pred) = pred_id {
                format!("EXISTS subplan[{}]", pred)
            } else {
                "EXISTS".to_string()
            }
        }
        Opcode::All(plan_id) => format!("ALL subplan[{}]", plan_id),
        Opcode::Jump(target) => format!("JUMP {}", target),
        Opcode::JumpIfEmpty(target) => format!("JUMP_IF_EMPTY {}", target),
        Opcode::JumpIfNotEmpty(target) => format!("JUMP_IF_NOT_EMPTY {}", target),
        Opcode::Iif(true_plan, false_plan, else_plan) => {
            if let Some(else_id) = else_plan {
                format!(
                    "IIF true[{}] false[{}] else[{}]",
                    true_plan, false_plan, else_id
                )
            } else {
                format!("IIF true[{}] false[{}]", true_plan, false_plan)
            }
        }
        Opcode::Return => "RETURN".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_visualization_formats() {
        let node = AstNode::IntegerLiteral(42);

        // Test all formats compile
        let _ = node.visualize(VisualizationFormat::Mermaid);
        let _ = node.visualize(VisualizationFormat::Dot);
        let ascii = node.visualize(VisualizationFormat::AsciiTree);

        assert!(ascii.contains("Integer: 42"));
    }
}
