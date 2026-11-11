//! Convert a sheet from any format this crate reads to any it writes.
//!
//! ```text
//! cargo run --example convert -- in.tnz out.xdts [out.ditis …]
//! ```
//!
//! The input is told apart by its contents, the outputs by their
//! extensions: `.xdts`, `.tdts`, `.sts`, and `.ditis` or `.json`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, outputs @ ..] = args.as_slice() else {
        eprintln!("usage: convert IN OUT [OUT …]");
        return ExitCode::FAILURE;
    };
    if outputs.is_empty() {
        eprintln!("usage: convert IN OUT [OUT …]");
        return ExitCode::FAILURE;
    }
    match run(input, outputs) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &str, outputs: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let sheet = xdts::from_bytes(&std::fs::read(input)?)?;
    let opts = xdts::WriteOptions::default();
    for out in outputs {
        let ext = out.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        let bytes = match ext.as_deref() {
            Some("xdts") => xdts::xdts::to_string(&sheet, &opts).into_bytes(),
            Some("tdts") => xdts::tdts::to_string(&sheet, &opts).into_bytes(),
            Some("ditis" | "json") => xdts::ditis::to_string(&sheet, &opts).into_bytes(),
            Some("sts") => xdts::sts::to_bytes(&sheet)?,
            _ => return Err(format!("{out}: no format for this extension").into()),
        };
        std::fs::write(out, bytes)?;
    }
    Ok(())
}
