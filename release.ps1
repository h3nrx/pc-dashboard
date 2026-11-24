# Builds the app and publishes it as a GitHub release.
#   .\release.ps1 -Version 1.1.0 -Notes notes.md            build + publish
#   .\release.ps1 -Version 1.1.0 -BuildOnly                 just build
param([Parameter(Mandatory)][string]$Version, [string]$Notes, [switch]$BuildOnly)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Version must look like 1.2.0' }
$root = $PSScriptRoot
$utf8 = New-Object Text.UTF8Encoding $false
function Set-Text($file, $pattern, $new) {
    $t = [IO.File]::ReadAllText($file)
    [IO.File]::WriteAllText($file, ([regex]$pattern).Replace($t, $new, 1), $utf8)
}
Set-Text "$root\src-tauri\Cargo.toml" '(?m)^version = "[^"]+"' "version = `"$Version`""
Set-Text "$root\src-tauri\tauri.conf.json" '"version": "[^"]+"' "`"version`": `"$Version`""

# local folder names must not end up inside the public exe
$env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\h3nrx-control-build"
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
$sep = [char]0x1f
$env:CARGO_ENCODED_RUSTFLAGS = @('-C', 'target-feature=+crt-static', "--remap-path-prefix=$cargoHome=cargo", "--remap-path-prefix=$env:CARGO_TARGET_DIR=target",
    "--remap-path-prefix=$root=src", "--remap-path-prefix=$env:USERPROFILE=home", '-C', 'link-arg=/PDBALTPATH:%_PDB%') -join $sep
(Get-Item "$root\src-tauri\build.rs").LastWriteTime = Get-Date
Push-Location "$root\src-tauri"
$ErrorActionPreference = 'Continue'
cargo build --release
$code = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
Pop-Location
if ($code) { throw 'build failed' }
New-Item -ItemType Directory -Force "$root\dist" | Out-Null
$exe = "$root\dist\PC-Dashboard.exe"
Copy-Item "$env:CARGO_TARGET_DIR\release\pc-dashboard.exe" $exe -Force
$text = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes($exe))
$wide = $text.Replace("$([char]0)", '')
foreach ($t in $text, $wide) { if ($t.IndexOf("\Users\$env:USERNAME", [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw 'the exe contains a local path' } }
"built $exe"
if ($BuildOnly) { return }

if (-not $Notes) { throw 'Notes file is missing' }
$short = $Version -replace '^(\d+\.\d+)\.0$', '$1'
Push-Location $root
try {
    $ErrorActionPreference = 'Continue'
    git add -A; git diff --cached --quiet
    if ($LASTEXITCODE) { git commit -q -m "V$short" -m 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>'; if ($LASTEXITCODE) { throw 'commit failed' } }
    git tag "v$Version"; if ($LASTEXITCODE) { throw 'tag failed' }
    git push -q origin main "v$Version"; if ($LASTEXITCODE) { throw 'push failed' }
    gh release create "v$Version" $exe --title "V$short" --notes-file $Notes --verify-tag; if ($LASTEXITCODE) { throw 'release failed' }
} finally { Pop-Location }
