use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Write generated modules to the given output directory.
/// Creates the directory if it does not exist.
pub fn write_modules(output_dir: &Path, modules: &HashMap<String, String>) -> Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("creating output directory {}", output_dir.display()))?;

    for (filename, contents) in modules {
        let path = output_dir.join(filename);
        fs::write(&path, contents)
            .with_context(|| format!("writing generated file {}", path.display()))?;
    }

    Ok(())
}
