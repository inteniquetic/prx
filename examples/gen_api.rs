//! Prints the generated API documents (T205). `make schema` runs it.
//!
//! `cargo run --example gen_api -- <dir>` writes `openapi.json` and
//! `prx.schema.json` into `<dir>`.

use std::{fs, path::PathBuf};

use prx::admin::doc;

fn main() -> std::io::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "docs".into()));
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("openapi.json"), doc::pretty(&doc::openapi()))?;
    fs::write(
        dir.join("prx.schema.json"),
        doc::pretty(&doc::config_schema()),
    )?;
    Ok(())
}
