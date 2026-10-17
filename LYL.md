# LiveYourLife HEMTT compatibility fork

Repository: https://github.com/bjoern-sobolewski/HEMTT

The `lyl-compat` branch merges upstream `main` without rewriting published
history. Release `v1.22.1-lyl.2` adds the PBO extraction security fix below.
Release `v1.22.1-lyl.1` includes upstream
`b888164616dbaf1007400485be4e4ee2b3521818` and the completed `v1.22.0-lyl.6`
compatibility/editor baseline. Keep the upstream remote and carry focused generic
compatibility patches; project rules belong in the consuming adapter.

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

Release `v1.22.0-lyl.5` retains failed include dependencies before tokenization,
so repairing a BOM or lexer error in a header refreshes every affected consumer.
The editor protocol regressions cover rejection and repair on shared SQF/config
headers, including a disk BOM hidden by VS Code's text model.

Release `v1.22.0-lyl.6` makes comment tokenization quote-aware. Wildcard URLs
such as `"https://example.test/*"` and quoted `//` or `/*...*/` remain string
data exactly once; subsequent config declarations are no longer swallowed.
Real line/block comments still follow their own delimiters, including an open
trailing block comment. This fix is shared by the CLI and language server.

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
should merge upstream deliberately and rerun both upstream tests and the
LiveYourLife worklist, preserving failures as failures.

## PBO extraction security (v1.22.1-lyl.2)

`utils pbo unpack` validates every archive entry before creating output. It
rejects traversal, rooted/drive/UNC paths, Windows alternate streams and device
names, and ambiguous trailing dots/spaces on every host. Extraction requires a
new output directory, rejects directory symlinks/junctions/reparse points, and
creates files exclusively. Archive entries cannot overwrite existing files,
generated metadata or `--derap` output. Both PBO separator styles remain supported.
The generic PBO reader still preserves archive names; the filesystem policy lives
in the unpack command. These checks do not sandbox a hostile local process that
concurrently changes filesystem paths.

Regressions in `bin/src/utils/pbo/unpack.rs` cover unsafe paths, preservation of
outside files, ordinary files/properties, derapification collisions, hard links
and directory links (a real junction on Windows). Run locally with Rust 1.99.1:
`cargo test -p hemtt -p hemtt-pbo --lib --tests --locked -- --nocapture --test-threads=1`.
This is filesystem/static tooling evidence. Consumers must use the immutable
`v1.22.1-lyl.2` release with matching source/archive/executable checksum pins;
prior release assets are retained and remain affected by this extraction issue.

Local Windows validation: all 23 CLI/PBO unit/integration tests pass, including
nine extraction-security regressions. Formatting and diff whitespace checks pass.

# Editor integration

The LYL VS Code client is maintained in the separate public
`bjoern-sobolewski/vscode-lyl-sqf` repository, reusing this fork's `hls` server, protocol, grammars and
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

`hemtt/config/structure` takes an exact config-root URL and returns the ordered
parsed class/property structure, literal string fields, external declarations,
inheritance and deletions, plus include dependencies. It uses the existing
preprocessor, parser and dirty source database; failed preprocessing/parsing
returns explicit limitations. A recovered AST with blocking analysis findings is
returned with explicit provisional findings, so unrelated config lint errors
cannot silently erase all discoverable registrations. The structural request
also balances and skips `__EXEC(...)` bodies without expanding or executing them.
Each skipped body makes the entire index provisional: side effects and dependent
expression values remain unknown. Unbalanced bodies still fail. Ordinary
preprocessing, diagnostics, verification and packaging retain their strict PE25
failure; the structural projection must never be used as validation output.
Macro calls with incorrect argument counts retain PE9 and become opaque,
nonliteral expressions only in the structural projection. This does not accept a
new macro signature, infer a value or execute code. An unclosed macro invocation
fails instead of being silently consumed at EOF. Other preprocessing errors
remain explicit failures of discovery.
An optional `classes` list permits recovery of selected top-level declarations
when unrelated config syntax prevents a full parse. Balanced statement boundaries
select the declarations, then the existing config parser parses them. Root parse
failures remain provisional findings; malformed boundaries or selected declarations
fail recovery. This does not validate omitted properties or resolve external bases.
The editor interprets CfgFunctions and resolves
project file paths outside this generic endpoint. Existing macro signature help
is unchanged; command parameter hints use the adapter's shared command contracts.

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

## Upstream main integration (2026-10-17)

The 21 upstream commits since v1.22.0 include guarded `isNil`/short-circuit S26
fixes, SQF array/if optimizations, shared compiler/lint performance work, forward
slash checks for includes and project paths, TOML lints and unknown lint-config
validation, nested config-array stringtable discovery, C19 notes, dependency
updates and editor path-copy support. These remain enabled. The merge retains
UTF-16 completion positions, overlays, include-consumer invalidation, diagnostic
scope ownership, prepared lint runners and both generic editor endpoints.

Focused merge regressions cover compact guarded short circuits (including array
expressions), dirty SQF/config TOML lints and repair without disk writes, invalid
TOML regex/empty patterns/targets, UTF-8 match spans, prepared manager sharing,
unknown lint configurations and UTF-16 completion with trailing whitespace.
Config raw TOML checks now live in the shared checker used by CLI and HLS;
SQF/config HLS checks read the source database rather than stale disk contents.
No project execution contexts or mission-specific parser rules were added.

Use the pinned Rust 1.99.1 toolchain: upstream uses APIs unavailable in the older
1.96.1 installation. Diagnostic fixtures require NO_COLOR to be unset. Run the
core libraries/HLS and supporting format libraries with `--lib --tests --release
--locked --target x86_64-pc-windows-msvc -- --nocapture --test-threads=1`.
Run CLI tests in the ordinary debug test profile: the release executable exits
on intentionally invalid fixtures, while debug tests return their error value.
The nocapture option is needed by upstream's own stdout-capture test. These are
local commands; GitHub Actions remains disabled. See the consuming extension's
VALIDATION.md/PERFORMANCE.md and the Arma maintenance report for measured static
findings, real-LSP/editor checks, performance and explicitly separate engine
coverage. Existing ignored upstream tests are retained, not counted as passes.

Publish immutable releases from clean exact commits with Windows binaries,
SHA256SUMS, LICENSE and a matching vendored corresponding-source bundle. Verify
public asset digests and provision new consumer caches before updating pins.
Never replace a published tag or asset. Keep full local Arma diagnostic/source
inventories private; public reports contain aggregate findings only.

Local Rust validation: 589 unit/integration tests passed across common, workspace,
preprocessor, config, SQF, HLS, CLI, LZO, PAA, PBO, P3D, stringtable, signing and
WSS. Two inherited tests remain ignored (moving wiki examples and a manual SQFC
A/B helper). The core documentation suites also pass their one runnable example;
its stale upstream `StdoutCapture` name was corrected to imported `OutputCapture`.
`cargo fmt --all -- --check` passes. No test or lint was disabled for this update.

The unchanged full Arma snapshot maps 4,018 sources, including ordered mission,
addon and shared mounts plus actual installed game headers. Staging reported no
limitations. Both the released lyl.6 CLI and this update fail before SQF compilation
with the same 2,189 annotation records. Raw comparison differs only in the order
of the CCHU expected-token alternatives; the set, source range, severity and
finding are unchanged. Do not interpret that failed config gate as full SQF
validation. Separate real-HLS and historical SQF performance workloads cover
additional SQF analysis; their precise scopes/results belong in the consuming
maintenance report. Original/staged sources and game archive hashes are retained
locally. This update adds no native-engine or multiplayer execution evidence.


## Saved-source analysis and compiler performance (v1.22.1-lyl.3)

The language-server binary also supports `--verify-stdio`: serialized JSON-line
requests with an absolute `workspace` and optional `reset` return a complete
SQF/config diagnostic inventory and explicit checked/reused/completed counts.
It checks config and SQF units independently; config errors never hide SQF
analysis. Infrastructure failures remain incomplete. This saved-source session
is separate from dirty editor buffers. No SQF is executed.

Every request compares source bytes, including missing/new include paths through
inventory changes. Source edits invalidate their units and recorded consumers;
header, project, rule, command metadata and file inventory changes conservatively
rebuild all units. Both snapshot boundaries must match. Processes expire under
the consumer's lifetime policy; no persistent on-disk result cache is trusted.

Profiling on the consuming project's valid 1,833-SQF/four-config frozen workload
identified lexing as the largest measured parser subphase. Common token dispatch
retains the original lexer for malformed/unusual input; UTF-8 character spans,
quoted delimiters, numeric forms and every diagnostic remain covered by a
12,000-case differential test and existing parser/native-compatibility fixtures.
`HEMTT_REFERENCE_LEXER=1` selects the retained reference implementation for
sequential comparisons using the same binary. Tied spelling suggestions now
sort lexically instead of selecting nondeterministic candidates from hash maps.
`HEMTT_PROFILE=1` enables aggregate preprocessing, lexing, grammar, inspection
and lint timers; ordinary analysis leaves profiling off.

Three alternating reference/optimized runs took median 8.306/4.726 seconds with
identical complete findings and unchanged source hashes. This is the narrower
valid-config compiler workload, not a passing production repository or native
Arma test. The consuming adapter retains its failed CLI configuration gate and
adds complete SQF/config analysis alongside it. See the matching editor's
PERFORMANCE.md for final end-to-end measurements and limitations.
