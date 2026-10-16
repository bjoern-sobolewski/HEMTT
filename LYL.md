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

Release `v1.22.0-lyl.4` supersedes the initial BOM acceptance: CLI preflight and
shared preprocessing reject leading UTF-8 BOMs with the `BOM` encoding-policy
diagnostic. This covers SQF, configs, included headers and unsaved HLS overlays.
Save source as UTF-8 without BOM. Native probes still establish that some SQF
loading paths accept BOMs; the conservative policy avoids depending on acceptance
across every loader/tool. Unicode inside strings remains supported, including
U+FEFF that is not a file's leading BOM. This generic encoding policy contains
no LiveYourLife execution-role rules.

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
# Editor integration

The LYL VS Code client is maintained in the LiveYourLife repository at
`tools/vscode-lyl-sqf`, reusing this fork's `hls` server, protocol, grammars and
language configurations. It adapts the repository's existing source mounts and
reviewed command-context checks outside the general parser. Its VSIX bundles
the Windows x64 language server with checksum/source pins and upstream notices.

This fork additionally fixes editor-only gaps: lint dirty buffers and dependent
includes, recheck config roots introduced after startup, restore diagnostics on
close/disk changes, use overlay content and UTF-16 columns in diagnostic spans,
accept full-buffer replacements and UTF-16 incremental edits, reject unrelated
workspace guesses, and merge config/SQF diagnostics without one clearing the
other. It adds native command completion alongside inherited macro/path
completion. The server's TCP startup protocol is unchanged.

An optional `.hemtt/command-reference.json` supplies a complete schema-1 command
snapshot in arma3-wiki's serde JSON format. When present, both CLI and HLS use
only that metadata plus explicit custom commands, without reading/updating an
ambient Git wiki cache. Malformed snapshots fail explicitly. This generic
workspace input lets the LYL adapter inject the same corrected offline reference
used by agent lookup and context checks. No LYL role or policy rule lives in the
general parser.

Build the language server with:

```powershell
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS='-C target-feature=+crt-static'
cargo build -p hemtt-language-server --release --locked --target x86_64-pc-windows-msvc
```

Run `cargo test -p hemtt-language-server -p hemtt-workspace -p hemtt-preprocessor
-p hemtt-sqf --locked` locally. On hosts inheriting `NO_COLOR`, clear it for
snapshot tests. The client has additional real-LSP and packaged VS Code tests.
Editor integration tests are static tooling tests, not native-engine evidence.

## Local performance work (2026-10-16)

The language server indexes reverse include consumers and replaces each
compilation unit's diagnostic scope atomically. It publishes only changed file
sets, retaining other config/SQF consumer findings and explicit repair clears.
Project changes recheck only the owning workspace. Failed preprocessing keeps
its dependency edges so header repairs are still analyzed.

The shared lint manager prepares enabled runners and effective configurations
once per compilation unit instead of allocating them for every AST node. The
same implementation is used by CLI and HLS; no lint or diagnostic category is
removed. Project configuration is read once per check, so subsequent checks
still observe configuration changes without a persistent stale config cache.

Set `HEMTT_PROFILE=1` before starting HLS and request `hemtt/performance` for
aggregate phase counts/times (no source contents). Disabled by default. Timers
include parallel work, lock waits and nested phases; their totals are not wall
time or process CPU. The LYL extension's `test/performance.js` drives a repeatable
real-repository workload, captures a Node CPU profile and process CPU/memory,
and checks dirty edits, shared headers, command hovers and context analysis.
Use its inventory comparator to ensure speed changes retain all findings.
