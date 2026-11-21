//! Command line front end.
//!
//! Deliberately thin: the CLI must not be able to do anything the library
//! cannot, or the two drift and the library stops being the contract.

use std::process::ExitCode;

/// `cic-primitive read <file.yaml>`: runs the read stage on the file and prints its size on success
/// or the rejection on failure (exit 1); a usage error or an unreadable file exits 2. `read` is the
/// only stage the CLI exposes so far.
fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: cic-primitive read <file.yaml>");
        return ExitCode::from(2);
    };
    if path != "read" {
        eprintln!("unknown command: {path}");
        eprintln!("the only stage implemented so far is `read`");
        return ExitCode::from(2);
    }
    let Some(file) = args.next() else {
        eprintln!("usage: cic-primitive read <file.yaml>");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(&file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {file}: {e}");
            return ExitCode::from(2);
        }
    };
    match cic_primitive_engine::reader::parse(
        &bytes,
        cic_primitive_engine::Stage::Read,
        "$",
        "document",
    ) {
        Ok(_) => {
            println!("read: OK  ({} bytes)", bytes.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
