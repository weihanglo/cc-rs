# Design: Inherit cargo `-Ztrim-paths` remap rules in cc-rs

Status: implemented, not yet submitted upstream.
This doc captures the design and rationale
so work can continue on another machine.
Drop this commit before sending the PR upstream.

## Problem

Internal ticket: [V2288414162] — since CargoBrazil migrated to `-Ztrim-paths`,
C code compiled via `cc` embeds absolute workspace paths through `__FILE__`,
breaking cache reuse across workspaces
(parent ticket: V2240613617, cross-workspace local caching).

Upstream ask: [rust-lang/cc-rs#593] — pass path remapping to C compilers.

## Cargo-side contract

Nightly cargo with the unstable `-Ztrim-paths` feature sets,
for build-script executions (`custom_build.rs`):

- `CARGO_TRIM_PATHS_SCOPE` —
  the resolved `trim-paths` profile value:
  `none`, `all`, or a comma-joined subset of `macro`, `diagnostics`, `object`.
- `CARGO_TRIM_PATHS_REMAP` —
  the exact `<from>=<to>` pairs cargo passes to rustc
  (package remap, build-dir remap, sysroot remap; see `trim_paths_remap()`
  in cargo's `core/compiler/mod.rs`),
  joined by the platform path separator.

The cargo unstable docs explicitly name cc's prefix-map flags
as the intended consumer of these variables.

## Design

### Flag mapping (GNU/Clang only)

For each remap pair:

- `macro` scope -> `-fmacro-prefix-map=<from>=<to>` (`__FILE__` and friends)
- `object` scope -> `-fdebug-prefix-map=<from>=<to>` (debug info)
- `all` -> both flags
- `diagnostics`, `none` -> nothing (no C compiler equivalent)

`all` deliberately expands to the two scope-specific flags
rather than `-ffile-prefix-map`:
identical effect for the scopes we can honor,
and each scope stays an independent function of the input.

MSVC is skipped entirely (no equivalent flag family).
clang-cl would need `/clang:` wrapping; deferred until there is demand.

### Compiler support probing

`-fdebug-prefix-map` is accepted by virtually every GCC/Clang in use
(GCC 4.8+, Clang 3.8+) and is emitted unprobed.
`-fmacro-prefix-map` needs GCC >= 8 / Clang >= 10,
so it is probed via `is_flag_supported_inner`
using the first real remap pair (result cached per compiler in `build_cache`).
On rejection: cargo warning ("`__FILE__` will not be remapped"),
macro flags skipped, build proceeds.

The probing sits in its own commits (see stack below)
so it can be dropped wholesale if upstream prefers unconditional emission.

### Probe isolation (load-bearing detail)

The probe internally constructs a fresh `Build` and calls `try_get_compiler`,
which itself calls `add_trim_paths_flags`.
The probe `Build` must set `.inherit_trim_paths(false)`
(mirroring the existing `.inherit_rustflags(false)`), otherwise:

1. infinite recursion: probe -> `try_get_compiler` -> probe -> ...
2. poisoning: trim-paths flags leak into every user `flag_if_supported` probe,
   so one unsupported flag would fail all other probes.

### API surface

`Build::inherit_trim_paths(bool)`, default `true`,
mirroring `inherit_rustflags` in name, shape, and default.
No other public API.
Doc comment states the cargo feature is unstable
and the env var contract (and this behavior) may evolve with it.

## Rationale for on-by-default

1. The value is in the default.
   Thousands of `cc`-using crates; per-crate opt-in makes the feature dead —
   a build is only as reproducible as its least-maintained `-sys` crate.
2. Tiny blast radius.
   The env vars only exist under nightly `-Ztrim-paths`;
   stable users see byte-identical behavior.
   For users who did opt in, ignoring the vars would be the surprising choice:
   cc would be unilaterally disregarding an explicit profile setting.
3. Control lives at the right layer (three tiers):
   - End user (primary package) owns the profile,
     including a per-dependency kill switch via
     `[profile.<name>.package.<dep>] trim-paths = "none"` —
     cargo resolves `unit.profile` per unit before setting the env vars,
     so an overridden dependency's build script never sees them.
     This is why no cc-specific end-user env var (`CC_NO_TRIM_PATHS`) is needed.
   - Crate author: `inherit_trim_paths(false)` for
     "my compiler setup is incompatible regardless of the user's profile".
   - cc-rs: the probe handles the involuntary case (old compiler) automatically.
4. Unstable-feature hedge: the bool opt-out stays meaningful
   even if the env var contract changes; minimal API commitment.

PR nuance to state explicitly:
what is inherited is the profile of the unit the build script runs *for*
(the dependency being compiled), not `build-override`
(which governs how build scripts themselves are compiled).

## Commit stack (jj, on top of main@origin = v1.2.67 / fa031a07)

C-TEST pattern: each test commit pins current behavior;
the following feature commit flips the assertions.
Every commit builds and passes tests independently (bisect-safe).

1. `ylpyyxyx` test: Add tests for cargo trim-paths env vars
   — `tests/trim_paths.rs`, asserts env vars are ignored (`must_not_have`).
2. `pmokzmmv` feat: Inherit path remap rules from cargo trim-paths
   — `add_trim_paths_flags`, `inherit_trim_paths`, probe isolation;
   flips assertions, adds `opt_out` test.
3. `zuvxmxuv` test: Emulate compiler family detection in cc-shim
   — shim answers `-E` with family pragma markers, rejects `-?` for GCC-like
   names. Without this, detection routed through the shim misidentifies it
   as MSVC (empty `-E` output + `-?` accepted). Pre-existing tests never
   noticed because without `CC_SHIM_OUT_DIR` in the *process* env the shim
   panics and cc-rs falls back to name-based family guessing.
4. `qrxswuws` test: Add test for unsupported macro remap flag
   — `CC_SHIM_FAIL_IF_ARG` simulates old GCC; asserts build currently fails.
5. `vlxzltuk` feat: Probe -fmacro-prefix-map support before emitting it
   — gating + warning; flips assertion to "probe fails, build succeeds".

Commits 4+5 (and optionally 3) are droppable if upstream rejects probing.

## Test-infra findings (hard-won, do not rediscover)

- Each file in `tests/` is a separate test binary — required here because
  these tests mutate process-global env; `GlobalEnv::lock()` only serializes
  within one binary.
- The flag-support probe re-resolves the compiler:
  `PATH` set via `Build::env` does NOT apply to the probe's process spawn,
  so PATH-shim tests probe the *system* `cc` instead
  (this machine: GCC 7.3 — which is exactly the compiler being gated for,
  and how the issue was found).
  Fix: pass the shim via `.compiler(<abs path>)`
  and set `CC_SHIM_OUT_DIR` process-wide.
- Detection/probe invocations are recorded by the shim and shift `out<N>`
  indices; tests locate the compile command by scanning for `foo.c`
  (`compile_cmd` helper) instead of hardcoding indices.
- MSRV is 1.63: no `let-else` (needs 1.65).
- `clippy --all-targets` errors in `src/target/{llvm,parser}.rs`
  (`std::env::var` disallowed) are pre-existing on main, not from this stack.

## Remaining work

- Open the upstream PR referencing cc-rs#593;
  cite the cargo unstable docs naming cc as the intended consumer.
  Expect discussion on: default-on, probing, MSVC/clang-cl scope.
- sccache: verify/implement exclusion of `-f*-prefix-map` args
  from the C compilation hash — currently NOT special-cased
  (owner confirmed; without it the fix just moves cache-key pollution
  from `__FILE__` into the command line).
- Internal short-term option (independent of upstream latency):
  CargoBrazil can inject `-ffile-prefix-map` via `CFLAGS`/`CXXFLAGS` today.

[V2288414162]: https://taskei.amazon.dev/tasks/V2288414162
[rust-lang/cc-rs#593]: https://github.com/rust-lang/cc-rs/issues/593
