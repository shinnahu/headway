# Builds the release executables and packs them into dist\ for a GitHub release.
#
# Rust normally writes source file locations into a program, and those contain
# your user folder and this checkout's path. Here both are replaced, so neither
# ends up inside what you publish. The script fails if it still finds them.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$sep = [char]0x1f
$env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~" + $sep + "--remap-path-prefix=$root=."

cargo build --release -p headway -p headway-mcp
if ($LASTEXITCODE -ne 0) { throw 'The build failed.' }

$exes = @("$root\target\release\headway.exe", "$root\target\release\headway-mcp.exe")
$needles = @($env:USERPROFILE, $root, $env:USERNAME) | Where-Object { $_ -and $_.Length -ge 3 }
foreach ($exe in $exes) {
    $bytes = [IO.File]::ReadAllBytes($exe)
    $ascii = [Text.Encoding]::ASCII.GetString($bytes)
    $wide = [Text.Encoding]::Unicode.GetString($bytes)
    foreach ($needle in $needles) {
        if ($ascii.IndexOf($needle, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or $wide.IndexOf($needle, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
            throw "$(Split-Path -Leaf $exe) still contains ""$needle""."
        }
    }
}

$dist = "$root\dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$zip = "$dist\Headway-$version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $exes -DestinationPath $zip
Copy-Item $exes $dist -Force

Get-ChildItem $dist | ForEach-Object { '{0,-36} {1,6:N1} MB' -f $_.Name, ($_.Length / 1MB) }
