# Re-syncs boltffi_cli/vendor/runtime-typescript from runtime/typescript.
#
# build.rs builds @boltffi/runtime from source so pack dart-web's vendored
# copy never goes stale -- but that only works from a full monorepo
# checkout, since a packaged/published boltffi_cli crate (cargo install,
# crates.io) can never contain a sibling ../runtime/typescript directory.
# This vendored copy is build.rs's fallback source for that case. It has
# no CI check keeping it in sync, so run this (and commit the result)
# whenever runtime/typescript/src changes.

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$CliDir = Resolve-Path (Join-Path $ScriptDir "..")
$RootDir = Resolve-Path (Join-Path $CliDir "..")
$SourceDir = Join-Path $RootDir "runtime/typescript"
$VendorDir = Join-Path $CliDir "vendor/runtime-typescript"

if (-not (Test-Path (Join-Path $SourceDir "src"))) {
    Write-Error "runtime/typescript/src not found at $SourceDir -- run this from a full boltffi monorepo checkout"
    exit 1
}

if (Test-Path $VendorDir) {
    Remove-Item -Recurse -Force $VendorDir
}
New-Item -ItemType Directory -Force -Path (Join-Path $VendorDir "src") | Out-Null
Copy-Item (Join-Path $SourceDir "src/*.ts") (Join-Path $VendorDir "src")
Copy-Item (Join-Path $SourceDir "package.json") (Join-Path $VendorDir "package.json")
Copy-Item (Join-Path $SourceDir "package-lock.json") (Join-Path $VendorDir "package-lock.json")
Copy-Item (Join-Path $SourceDir "tsconfig.json") (Join-Path $VendorDir "tsconfig.json")

Write-Host "Verifying the vendored copy builds on its own..."
Push-Location $VendorDir
try {
    npm install
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build failed" }
}
finally {
    Pop-Location
}
Remove-Item -Recurse -Force (Join-Path $VendorDir "dist") -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force (Join-Path $VendorDir "node_modules") -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "Synced $VendorDir from $SourceDir."
Write-Host "Review the diff and commit boltffi_cli/vendor/runtime-typescript."
