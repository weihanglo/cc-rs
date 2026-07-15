//! Tests for inheriting path remap information from cargo's unstable
//! `-Ztrim-paths` feature, communicated to build scripts via the
//! `CARGO_TRIM_PATHS_SCOPE` and `CARGO_TRIM_PATHS_REMAP` environment
//! variables.
//!
//! This test is in its own module because it modifies the environment and
//! would affect other tests when run in parallel with them.
#![cfg(not(windows))]

mod support;

use crate::support::Test;

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

    test.gcc().file("foo.c").compile("foo");

    let cmd = test.cmd(0);
    for flag in MACRO_FLAGS.iter().chain(OBJECT_FLAGS) {
        cmd.must_have(flag);
    }
}

#[test]
fn scope_macro() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "macro");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);

    test.gcc().file("foo.c").compile("foo");

    let cmd = test.cmd(0);
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

    test.gcc().file("foo.c").compile("foo");

    let cmd = test.cmd(0);
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
/// the flag currently fails the build, as remap flags are emitted without
/// probing for compiler support.
#[test]
fn unsupported_macro_flag() {
    let mut test = Test::gnu();
    test.env.set("CARGO_TRIM_PATHS_SCOPE", "macro");
    test.env.set("CARGO_TRIM_PATHS_REMAP", REMAP);
    // Simulate a compiler that errors out on the remap flag.
    test.env.set("CC_SHIM_FAIL_IF_ARG", MACRO_FLAGS[0]);

    let result = test.gcc().file("foo.c").try_compile("foo");

    assert!(result.is_err());
}
