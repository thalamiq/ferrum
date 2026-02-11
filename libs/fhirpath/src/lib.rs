//! FHIRPath Engine - Production-ready implementation with AST → HIR → VM architecture
//!
//! This crate provides a complete FHIRPath evaluation engine following the architecture:
//! 1. **Parser** → AST (Abstract Syntax Tree)
//! 2. **AST** → HIR (High-level Intermediate Representation)
//! 3. **HIR** → VM (Virtual Machine with bytecode)
//!
//! # Architecture Overview
//!
//! ```text
//! Expression String
//!      |
//!   Parser -> AST
//!      |
//! Semantic Analysis -> HIR (typed, optimized)
//!      |
//! Code Generation -> VM Plan (bytecode)
//!      |
//! VM Execution -> Result Collection
//! ```

pub mod analyzer;
pub mod ast;
pub mod codegen;
pub mod context;
pub mod conversion;
pub mod engine;
pub mod error;
pub mod functions;
pub mod hir;
pub mod lexer;
pub mod parser;
pub mod resolver;
mod temporal_parse;
pub mod token;
pub mod typecheck;
pub mod types;
pub mod value;
pub mod variables;
pub mod visualize;
pub mod vm;

// Re-export main types
pub use context::Context;
pub use conversion::{zunder_fhirpath_value_to_json, ToJson};
pub use engine::{CompileOptions, Engine, EvalOptions, PipelineVisualization};
pub use error::{Error, Result};
pub use resolver::ResourceResolver;
pub use value::{Collection, Value};
pub use visualize::{VisualizationFormat, Visualize};
