param([string]$ToolRoot = (Join-Path $env:TEMP 'spaceless-api-tools'))
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (Split-Path $PSScriptRoot -Parent)
if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
    if (!(Test-Path -LiteralPath (Join-Path $ToolRoot 'cargo/bin/cargo.exe'))) { throw 'Install Rust and a supported C/C++ toolchain first.' }
    $env:CARGO_HOME = Join-Path $ToolRoot 'cargo'
    $env:RUSTUP_HOME = Join-Path $ToolRoot 'rustup'
    $env:PATH = (Join-Path $ToolRoot 'w64devkit/bin') + ';' + (Join-Path $ToolRoot 'cargo/bin') + ';' + $env:PATH
    $env:RUSTFLAGS = '-Lnative=' + (Join-Path $ToolRoot 'rust-link-support') + ' -Clink-arg=-Wl,--stack,16777216'
}
cargo test --manifest-path server/Cargo.toml --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo build --manifest-path server/Cargo.toml --locked
exit $LASTEXITCODE
