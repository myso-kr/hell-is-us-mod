# Builds a release zip: dist\hiumod-<version>.zip
#   hiumod.exe            the panel, overlays and CLI (assets are built in)
#   survey\               the survey tool, published framework-dependent (.NET 8 runtime)
#   README.md, LICENSE, NOTICE, THIRD-PARTY.md, CHANGELOG.md
# Nothing from the game goes in: no AES key, no mappings, no survey output, no Oodle.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$stage = Join-Path $root "dist\hiumod-$version"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null

cargo test --quiet
if ($LASTEXITCODE -ne 0) { throw 'tests failed' }
cargo build --release --quiet
if ($LASTEXITCODE -ne 0) { throw 'build failed' }
Copy-Item target\release\hiumod.exe $stage

dotnet publish tools\survey -c Release -o (Join-Path $stage 'survey') --self-contained false -v quiet
if ($LASTEXITCODE -ne 0) { throw 'survey publish failed' }
# The published folder must hold only the tool and its libraries.
Get-ChildItem (Join-Path $stage 'survey') -Recurse -Include *.pdb, *.usmap, *.json -Exclude *.deps.json, *.runtimeconfig.json |
    Remove-Item -Force

foreach ($f in 'README.md', 'LICENSE', 'NOTICE', 'THIRD-PARTY.md', 'CHANGELOG.md') { Copy-Item $f $stage }

$zip = "$stage.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Write-Host "packaged $zip"
