//! Taphonomy command-line interface.
//!
//! The CLI is an interface to the library. It parses arguments, calls into
//! the library, and formats output. It contains no analysis logic.
//!
//! See `docs/development/RESEARCH_LOG.md` conclusion 6.

use std::process::ExitCode;

use taphonomy::EvidenceFile;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);

    let Some(path) = args.next() else {
        eprintln!("usage: taphonomy <evidence-image>");
        return ExitCode::from(2);
    };

    if args.next().is_some() {
        eprintln!("error: expected exactly one path");
        return ExitCode::from(2);
    }

    match inspect(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn inspect(path: &std::ffi::OsStr) -> Result<(), taphonomy::Error> {
    let mut evidence = EvidenceFile::open(path)?;
    let reported = evidence.reported_size();
    let result = evidence.digest()?;

    println!("path         {}", evidence.path().display());
    println!("size         {reported} bytes");
    println!("read         {} bytes", result.bytes_read);
    println!("sha256       {}", result.digest);

    if result.bytes_read != reported {
        println!();
        println!("WARNING: bytes read does not match reported size.");
        println!("The digest covers {} bytes only.", result.bytes_read);
    }

    Ok(())
}
