param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Args
)

# flview helper for Windows + MSYS2 GNU toolchain.
# - Ensures MSYS2 MinGW64 is on PATH (gcc/ld/etc)
# - Uses the Rust GNU toolchain installed via rustup

$ErrorActionPreference = 'Stop'

$msysBin = 'C:\msys64\mingw64\bin'
if (-not (Test-Path -LiteralPath $msysBin)) {
    throw "MSYS2 MinGW64 bin not found at $msysBin. Install MSYS2 to C:\\msys64 and the mingw-w64-x86_64-toolchain package."
}

if (-not ($env:Path -split ';' | Where-Object { $_ -eq $msysBin })) {
    $env:Path = "$msysBin;" + $env:Path
}

$toolchain = 'stable-x86_64-pc-windows-gnu'

if (-not $Args -or $Args.Count -eq 0) {
    # Default to TUI mode.
    & cargo +$toolchain run -- tui
    exit $LASTEXITCODE
}

switch -Regex ($Args[0]) {
    '^(tui)$' {
        & cargo +$toolchain run -- tui
    }
    '^(web)$' {
        & cargo +$toolchain run -- web --addr 127.0.0.1:8080
    }
    default {
        # Pass-through to cargo. Examples:
        #   .\flview.ps1 build
        #   .\flview.ps1 run -- tui
        #   .\flview.ps1 run -- web --addr 0.0.0.0:8080
        & cargo +$toolchain @Args
    }
}

exit $LASTEXITCODE
