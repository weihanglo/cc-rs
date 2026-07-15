//! This bin target is only used for this crate's tests.
//! It is not intended for users and is not published with the library code to crates.io.

#![cfg_attr(test, allow(dead_code))]
#![allow(clippy::disallowed_methods)]

use std::env;
use std::fs::File;
use std::io::{self, prelude::*};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<_>>();
    let mut args = args.iter();
    let program = args.next().expect("Unexpected empty args");

    let out_dir = PathBuf::from(
        env::var_os("CC_SHIM_OUT_DIR")
            .unwrap_or_else(|| panic!("{}: CC_SHIM_OUT_DIR not found", program)),
    );

    // Find the first nonexistent candidate file to which the program's args can be written.
    let candidate = (0..).find_map(|i| {
        let candidate = out_dir.join(format!("out{i}"));

        if candidate.exists() {
            // If the file exists, commands have already run. Try again.
            None
        } else {
            Some(candidate)
        }
    }).unwrap_or_else(|| panic!("Cannot find the first nonexistent candidate file to which the program's args can be written under out_dir '{}'", out_dir.display()));

    // Create a file and record the args passed to the command.
    let f = File::create(&candidate).unwrap_or_else(|e| {
        panic!(
            "{}: can't create candidate: {}, error: {}",
            program,
            candidate.display(),
            e
        )
    });
    let mut f = io::BufWriter::new(f);

    (|| {
        for arg in args.clone() {
            writeln!(f, "{arg}")?;
        }

        f.flush()?;

        let mut f = f.into_inner()?;
        f.flush()?;
        f.sync_all()
    })()
    .unwrap_or_else(|e| {
        panic!(
            "{}: can't write to candidate: {}, error: {}",
            program,
            candidate.display(),
            e
        )
    });

    if program.starts_with("clang") {
        // Validate that we got no `-?` without a preceding `--driver-mode=cl`. Compiler family
        // detection depends on this.
        if let Some(cl_like_help_option_idx) = args.clone().position(|a| a == "-?") {
            let has_cl_clang_driver_before_cl_like_help_option = args
                .clone()
                .take(cl_like_help_option_idx)
                .rev()
                .find_map(|a| a.strip_prefix("--driver-mode="))
                .is_some_and(|a| a == "cl");
            if has_cl_clang_driver_before_cl_like_help_option {
                return ExitCode::SUCCESS;
            } else {
                eprintln!(
                    "Found `-?` argument, but it was not preceded by a `--driver-mode=cl` argument."
                );
                return ExitCode::FAILURE;
            }
        }
    }

    // Emulate the real compilers that compiler family detection runs against:
    // - `-E <detect_compiler_family.c>` prints the `#pragma message` markers
    //   the detection looks for on stdout.
    // - GCC-like compilers reject the cl-style `-?` help flag.
    // Without this, detection through the shim would misidentify it as MSVC
    // (empty `-E` output, `-?` accepted).
    let file_name = PathBuf::from(program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if file_name.starts_with("gcc") || file_name.starts_with("cc") || file_name.starts_with("clang")
    {
        if args.clone().any(|a| a == "-?") {
            eprintln!("{program}: unrecognized command-line option '-?'");
            return ExitCode::FAILURE;
        }
        if args.clone().any(|a| a == "-E") {
            if file_name.starts_with("clang") {
                println!("#pragma message \"clang\"");
            }
            println!("#pragma message \"gcc\"");
            return ExitCode::SUCCESS;
        }
    }

    // Allow tests to make the shim fail when a specific arg is present.
    if let Some(fail_arg) = env::var("CC_SHIM_FAIL_IF_ARG").ok() {
        if args.any(|a| a == &fail_arg) {
            eprintln!("{program}: simulated failure for arg '{fail_arg}'");
            return ExitCode::FAILURE;
        }
    }

    // Create a file used by some tests.
    let path = &out_dir.join("libfoo.a");
    File::create(path).unwrap_or_else(|e| {
        panic!(
            "{}: can't create libfoo.a: {}, error: {}",
            program,
            path.display(),
            e
        )
    });

    ExitCode::SUCCESS
}
