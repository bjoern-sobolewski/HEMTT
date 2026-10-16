# LiveYourLife HEMTT fork

Read LYL.md before editing. Maintain `lyl-compat` by merging upstream `main`;
do not rewrite published history. Check active chats and pending changes before
using the shared checkout. Coordinate releases with the separate
`vscode-lyl-sqf` and Arma repositories; never commit another chat's unfinished work.

Keep mission layout, execution contracts and project-specific policy in the Arma
adapter. Preserve generic parser compatibility, command-reference snapshots, BOM
policy, source overlays, failed include dependencies, UTF-16 positions, incremental
diagnostic ownership and the provisional config-structure endpoint. Discovery
output is never validation evidence.

Use Rust 1.99.1 or a verified compatible newer toolchain for upstream main's APIs.
The local release uses the locked dependencies and static Windows x64 CRT. Clear
inherited NO_COLOR for Rust diagnostic snapshots. Run relevant upstream suites,
all fork compatibility regressions, actual-HLS and packaged editor tests, full
Arma analysis and sequential before/after performance workloads. Investigate
diagnostic differences; never suppress findings to obtain a pass.

Use local verification only. Keep GitHub Actions disabled; do not publish to the
Marketplace. Release new versions without replacing tags/assets. Retain source
commit pins, archive/executable checksums, GPL corresponding source, vendored
dependency source and notices. Verify public downloads with fresh consumer caches
before recording new Arma pins. Keep mission sources, game headers and local
diagnostic inventories out of this public repository. Distinguish static tooling,
SQF-VM, native engine and multiplayer evidence in validation reports.
