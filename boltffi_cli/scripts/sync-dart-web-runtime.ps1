# Re-syncs boltffi_cli/vendor/runtime-js from runtime/typescript, transpiled.
#
# build.rs embeds this vendored JS so a packaged boltffi_cli crate
# (cargo install, crates.io) always has it on hand -- Cargo can never
# package a sibling ../runtime/typescript directory. There's no CI
# check keeping the copy in sync, so run this (and commit the result)
# whenever runtime/typescript/src changes.

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$CliDir = Resolve-Path (Join-Path $ScriptDir "..")
$RootDir = Resolve-Path (Join-Path $CliDir "..")
$SourceDir = Join-Path $RootDir "runtime/typescript"
$VendorDir = Join-Path $CliDir "vendor/runtime-js"

if (-not (Test-Path (Join-Path $SourceDir "src"))) {
    Write-Error "runtime/typescript/src not found at $SourceDir -- run this from a full boltffi monorepo checkout"
    exit 1
}

Write-Host "Building runtime/typescript..."
Push-Location $SourceDir
try {
    npm install
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build failed" }
}
finally {
    Pop-Location
}

if (Test-Path $VendorDir) {
    Remove-Item -Recurse -Force $VendorDir
}
New-Item -ItemType Directory -Force -Path $VendorDir | Out-Null
Copy-Item (Join-Path $SourceDir "dist/*.js") $VendorDir

Write-Host ""
Write-Host "Synced $VendorDir from $SourceDir."
Write-Host "Review the diff and commit boltffi_cli/vendor/runtime-js."
