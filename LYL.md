# LiveYourLife HEMTT compatibility fork

Repository: https://github.com/bjoern-sobolewski/HEMTT

The `lyl-compat` branch starts at upstream `v1.22.0`. Releases are named
`v1.22.0-lyl.N`. Keep the upstream remote and carry focused compatibility patches
instead of changing mission source to satisfy a different parser.

The first patch set adds native-compatible compact tokens, comma-separated code
statements, Unicode separators/control characters inside strings, UTF-8 BOM SQF
sources and open trailing block comments during file preprocessing. The native
reference is Arma 3 2.22.154089. Literal `#if __has_include("path")` and
`#if __has_include(<path>)` evaluate against the actual workspace/include mounts;
arbitrary expressions and macro-built paths are rejected. The existing ignore
pragma retains its explicit false-branch behavior. Files using this conditional
remain marked as unsuitable for rapification, so packaging cannot accidentally
freeze a deployment-dependent include decision.

A fallback entry recognizes the native `ctrlWebBrowserAction` outer signature
when the upstream command database lacks it. Its general return type is unknown:
native `Deflate` returned String although `supportInfo` reported NOTHING.
Availability does not establish locality, scheduling or every action's contract.
Project `.hemtt/commands` overrides and future upstream definitions still apply.

The fork also copies the documented `setDamage` contract to its legacy
`setDammage` alias (scraped as nular upstream), and corrects `addWeapon`'s
`autoselect` parameter from String to Boolean when that specific bad field is
present. Other metadata, including restrictions and optional parameters, stays
with the canonical command definition.

No argument, formatting, reserved-variable or broken-command lint is disabled.
Compiler acceptance does not prove correct runtime behavior. Missing includes,
remaining parser limitations and metadata errors must still be reported.

## Local validation

On Windows, upstream diagnostic fixtures contain ANSI colors. Clear `NO_COLOR`
in the test process before running:

```powershell
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue
cargo test -p hemtt-preprocessor -p hemtt-sqf --locked
```

`libs/preprocessor/tests/native_compatibility.rs` and
`libs/sqf/tests/native_compatibility.rs` contain the accepted/rejected regression
cases. The separate LiveYourLife repository records full-worklist and native
engine evidence in its SQF command verification guide. These Rust tests alone do
not validate production configs, UI, scheduling or multiplayer behavior.

Release `v1.22.0-lyl.1`: 239 Rust tests passed, one upstream test ignored. The
1,833-source LiveYourLife worklist reached analysis including all 85 BOM sources,
with real installed game headers staged. It still failed with 105 blocking
diagnostics; additional parsed files expose further argument/format/policy
findings. There were no remaining BOM, Unicode or `__has_include` diagnostics.
This is not a clean-repository or per-file passing claim.

## Windows release and consumption

Run `scripts/build-lyl-release.ps1` with Rust, the MSVC C++ build tools and Windows
SDK installed. It uses the checked-in Cargo lockfile, compiles the x64 executable
with a static CRT, creates `releases/windows-x64.zip`, and writes SHA-256 hashes.
Review the source/tests, commit and tag that exact source, then publish the archive
and checksums in the corresponding GitHub release. No CI runner is required.

LiveYourLife consumers install the release through their existing checksum-pinned
dependency manifest. They do not need Rust, an SDK, administrator access or PATH
changes. Archive/executable hashes and the source commit must be updated together;
never point a manifest at an untested or unpublished asset.

Do not put mission code, game headers or private game data into this public fork.
Keep upstream copyright/license files with distributed binaries. Future updates
should rebase/merge upstream deliberately and rerun both upstream tests and the
LiveYourLife worklist, preserving failures as failures.
