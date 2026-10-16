param()
$ErrorActionPreference = 'Stop'
$forkRoot = Split-Path -Parent $PSScriptRoot
$savedEnvironment = @{}
foreach ($name in @('CI', 'RELEASE', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS')) {
    $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
Push-Location $forkRoot
try {
    $env:CI = '1'
    $env:RELEASE = '1'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
    & cargo build -p hemtt --release --locked --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "Cargo release build failed ($LASTEXITCODE)." }
    $binary = Join-Path $forkRoot 'target/x86_64-pc-windows-msvc/release/hemtt.exe'
    & $binary --version
    if ($LASTEXITCODE -ne 0) { throw 'Built HEMTT could not report its version.' }
    $releaseDirectory = Join-Path $forkRoot 'releases'
    New-Item -ItemType Directory -Path $releaseDirectory -Force | Out-Null
    $archive = Join-Path $releaseDirectory 'windows-x64.zip'
    $license = Join-Path $forkRoot 'LICENSE'
    if (!(Test-Path -LiteralPath $license)) { throw 'Upstream LICENSE is missing.' }
    Compress-Archive -LiteralPath @($binary, $license, (Join-Path $forkRoot 'LYL.md')) -DestinationPath $archive -Force
    $archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    $binaryHash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    "$archiveHash  windows-x64.zip`n$binaryHash  hemtt.exe" | Set-Content -LiteralPath (Join-Path $releaseDirectory 'SHA256SUMS') -Encoding Ascii
    Write-Output "Archive: $archive"
    Write-Output "Archive SHA-256: $archiveHash"
    Write-Output "Executable SHA-256: $binaryHash"
}
finally {
    Pop-Location
    foreach ($name in $savedEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process')
    }
}
