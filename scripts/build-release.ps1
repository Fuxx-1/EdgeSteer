param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $CargoArguments
)

$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$flags = @()
if ($env:CARGO_ENCODED_RUSTFLAGS) {
    $flags += $env:CARGO_ENCODED_RUSTFLAGS -split [char]0x1f
}

function Add-PathMapping {
    param(
        [string] $Source,
        [string] $Destination
    )

    if ($Source) {
        $script:flags += "--remap-path-prefix=$Source=$Destination"
    }
}

Add-PathMapping $env:USERPROFILE "/build-home"

if ($env:CARGO_HOME) {
    Add-PathMapping $env:CARGO_HOME "/cargo"
} elseif ($env:USERPROFILE) {
    Add-PathMapping (Join-Path $env:USERPROFILE ".cargo") "/cargo"
}

Add-PathMapping $repositoryRoot "/edgesteer"

$env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f
Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
$env:EDGESTEER_LIVE_MANIFEST_PATH = "/edgesteer"

Push-Location $repositoryRoot
try {
    & cargo build --locked --release @CargoArguments
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
