//! Tests for inheriting path remap information from cargo's unstable
//! `-Ztrim-paths` feature, communicated to build scripts via the
//! `CARGO_TRIM_PATHS_SCOPE` and `CARGO_TRIM_PATHS_REMAP` environment
//! variables.
//!
//! This test is in its own module because it modifies the environment and
//! would affect other tests when run in parallel with them.
#![cfg(not(windows))]

mod support;

use crate::support::{Execution, Test};

/// Find the command that actually compiles `foo.c`.
///
/// The compiler-family detection and the `-fmacro-prefix-map` support probe
/// are also recorded by the compiler shim, so the compile command's index is
/// not stable.
fn compile_cmd(test: &Test) -> Execution {
    let mut i = 0;
    while test.td.path().join(format!("out{i}")).exists() {
        let cmd = test.cmd(i);
        if cmd.args.iter().any(|arg| arg.ends_with("foo.c")) {
            return cmd;
        }
        i += 1;
    }
    panic!("no command compiling foo.c was recorded");
}

/// `<from>=<to>` pairs as cargo passes them: joined by the platform path
/// separator (`:` on non-Windows, hence the `cfg` above).
const REMAP: &str =
    "/path/to/pkg=foo-0.1.0:/path/to/sysroot/lib/rustlib/src/rust=/rustc/1234567890abcdef";

const MACRO_FLAGS: &[&str] = &[
    "-fmacro-prefix-map=/path/to/pkg=foo-0.1.0",
    "-fmacro-prefix-map=/path/to/sysroot/lib/rustlib/src/rust=/rustc/1234567890abcdef",
];

const OBJECT_FLAGS: &[&str] = &[
    "-fdebug-prefix-map=/path/to/pkg=foo-0.1.0",
    "-fdebug-prefix-map=/path/to/sysroot/lib/rustlib/src/rust=/rustc/1234567890abcdef",
];

#[test]
fn scope_all() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "all");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);
    // The `-fmacro-prefix-map` support probe re-resolves the compiler; point
    // it at the shim explicitly (and let the probe find the shim's out dir
    // through the process environment) to keep the test hermetic.
    test.env.set("CC_SHIM_OUT_DIR", test.td.path());
    let shim = test.td.path().join("cc");

    test.gcc().compiler(shim).file("foo.c").compile("foo");

    let cmd = compile_cmd(&test);
    for flag in MACRO_FLAGS.iter().chain(OBJECT_FLAGS) {
        cmd.must_have(flag);
    }
}

#[test]
fn scope_macro() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "macro");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);
    test.env.set("CC_SHIM_OUT_DIR", test.td.path());
    let shim = test.td.path().join("cc");

    test.gcc().compiler(shim).file("foo.c").compile("foo");

    let cmd = compile_cmd(&test);
    for flag in MACRO_FLAGS {
        cmd.must_have(flag);
    }
    for flag in OBJECT_FLAGS {
        cmd.must_not_have(flag);
    }
}

#[test]
fn scope_object() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "object");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);

    test.gcc().file("foo.c").compile("foo");

    let cmd = test.cmd(0);
    for flag in OBJECT_FLAGS {
        cmd.must_have(flag);
    }
    for flag in MACRO_FLAGS {
        cmd.must_not_have(flag);
    }
}

/// `diagnostics` has no C compiler equivalent; combined with `macro` only
/// the macro remap flags apply.
#[test]
fn scope_macro_and_diagnostics() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "diagnostics,macro");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);
    test.env.set("CC_SHIM_OUT_DIR", test.td.path());
    let shim = test.td.path().join("cc");

    test.gcc().compiler(shim).file("foo.c").compile("foo");

    let cmd = compile_cmd(&test);
    for flag in MACRO_FLAGS {
        cmd.must_have(flag);
    }
    for flag in OBJECT_FLAGS {
        cmd.must_not_have(flag);
    }
}

/// `none` disables path sanitization; no remap flags should ever be emitted.
#[test]
fn scope_none() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "none");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);

    test.gcc().file("foo.c").compile("foo");

    let cmd = test.cmd(0);
    for flag in MACRO_FLAGS.iter().chain(OBJECT_FLAGS) {
        cmd.must_not_have(flag);
    }
}

/// Without the cargo-provided env vars nothing is emitted.
#[test]
fn no_env_vars() {
    let mut test = Test::gnu();
    test.env.remove("CARGO_TRIM_PATHS_SCOPE");
    test.env.remove("CARGO_TRIM_PATHS_REMAP");

    test.gcc().file("foo.c").compile("foo");

    let cmd = test.cmd(0);
    for flag in MACRO_FLAGS.iter().chain(OBJECT_FLAGS) {
        cmd.must_not_have(flag);
    }
}

/// `Build::inherit_trim_paths(false)` opts out of the inheritance.
#[test]
fn opt_out() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "all");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);

    test.gcc()
        .inherit_trim_paths(false)
        .file("foo.c")
        .compile("foo");

    let cmd = test.cmd(0);
    for flag in MACRO_FLAGS.iter().chain(OBJECT_FLAGS) {
        cmd.must_not_have(flag);
    }
}

/// `-fmacro-prefix-map` needs GCC >= 8 or Clang >= 10. A compiler rejecting
/// the flag fails the support probe; the macro remap flags are then skipped
/// and the build still succeeds.
#[test]
fn unsupported_macro_flag() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "macro");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);
    test.env.set("CC_SHIM_OUT_DIR", test.td.path());
    // Simulate a compiler that errors out on the remap flag.
    test.env.set("CC_SHIM_FAIL_IF_ARG", MACRO_FLAGS[0]);
    let shim = test.td.path().join("cc");

    let result = test.gcc().compiler(shim).file("foo.c").try_compile("foo");

    assert!(result.is_ok());
    let cmd = compile_cmd(&test);
    for flag in MACRO_FLAGS.iter().chain(OBJECT_FLAGS) {
        cmd.must_not_have(flag);
    }
}
