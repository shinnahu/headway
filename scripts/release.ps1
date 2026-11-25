# Builds what a release consists of: one file, dist\Headway-Setup-<version>.exe.
#
#   1. checks first: the same version everywhere, a complete Spanish
#      translation, the tests (skip with -SkipChecks)
#   2. builds headway.exe
#   3. builds the setup, with that headway.exe packed inside it
#   4. makes sure neither of them contains this computer's names
#
# About 4: Rust normally writes source file locations into a program, and those
# contain your user folder and this checkout's path. Here both are replaced, so
# neither ends up inside what you publish. The script fails if it still finds them.

param([switch]$SkipChecks)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Invoke-Step([string]$what, [scriptblock]$command) {
    & $command
    if ($LASTEXITCODE -ne 0) { throw "$what failed." }
}

$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
foreach ($conf in 'src-tauri\tauri.conf.json', 'crates\setup\tauri.conf.json') {
    $there = (Get-Content "$root\$conf" -Raw | ConvertFrom-Json).version
    if ($there -ne $version) { throw "$conf says version $there, but Cargo.toml says $version." }
}

if (-not $SkipChecks) {
    if (Get-Command node -ErrorAction SilentlyContinue) {
        Invoke-Step 'The translation check' { node scripts\check-i18n.mjs }
    } else {
        Write-Warning 'node is not installed, so the translation check was skipped.'
    }
    Invoke-Step 'The tests' { cargo test --workspace }
}

$sep = [char]0x1f
$env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~" + $sep + "--remap-path-prefix=$root=."

Invoke-Step 'Building Headway' { cargo build --release -p headway }
$app = "$root\target\release\headway.exe"

# The setup carries the program inside it: its build script packs the file named here.
$env:HEADWAY_PAYLOAD = $app
try {
    Invoke-Step 'Building the setup' { cargo build --release -p headway-setup }
} finally {
    Remove-Item Env:HEADWAY_PAYLOAD
}
$setup = "$root\target\release\headway-setup.exe"

$needles = @($env:USERPROFILE, $root, $env:USERNAME) | Where-Object { $_ -and $_.Length -ge 3 }
foreach ($exe in $app, $setup) {
    $bytes = [IO.File]::ReadAllBytes($exe)
    $ascii = [Text.Encoding]::ASCII.GetString($bytes)
    $wide = [Text.Encoding]::Unicode.GetString($bytes)
    foreach ($needle in $needles) {
        if ($ascii.IndexOf($needle, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or $wide.IndexOf($needle, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
            throw "$(Split-Path -Leaf $exe) still contains ""$needle""."
        }
    }
}
# The setup must be bigger than a setup with nothing inside (about 3 MB).
if ((Get-Item $setup).Length -lt (Get-Item $app).Length / 4) { throw 'The setup looks empty: headway.exe is not inside it.' }

$dist = "$root\dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$out = "$dist\Headway-Setup-$version.exe"
Copy-Item $setup $out -Force

'{0}  {1:N1} MB  (headway.exe inside: {2:N1} MB)' -f (Split-Path -Leaf $out), ((Get-Item $out).Length / 1MB), ((Get-Item $app).Length / 1MB)
'SHA-256  ' + (Get-FileHash $out -Algorithm SHA256).Hash.ToLower()
