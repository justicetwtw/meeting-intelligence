[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$script:CurrentStage = "initialization"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $repositoryRoot

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)]
        [string] $FilePath,

        [Parameter()]
        [string[]] $Arguments = @()
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE`: $FilePath $($Arguments -join ' ')"
    }
}

function Invoke-Stage {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name,

        [Parameter(Mandatory = $true)]
        [scriptblock] $Action
    )

    $script:CurrentStage = $Name
    Write-Host ""
    Write-Host "==> $Name"
    & $Action
}

function Resolve-Executable {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name,

        [Parameter()]
        [string[]] $FallbackPaths = @()
    )

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    foreach ($candidate in $FallbackPaths) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }

    throw "Required executable was not found: $Name"
}

function Import-MsvcEnvironment {
    if ($env:OS -ne "Windows_NT") {
        throw "scripts/verify.ps1 is the authoritative Windows verifier and requires Windows."
    }

    if ($null -ne (Get-Command link.exe -ErrorAction SilentlyContinue)) {
        return
    }

    $vswhereCandidates = @(
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe",
        "$env:ProgramFiles\Microsoft Visual Studio\Installer\vswhere.exe"
    )
    $vswhere = $vswhereCandidates |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1

    if ([string]::IsNullOrWhiteSpace($vswhere)) {
        throw "Visual Studio Build Tools were not found (vswhere.exe is missing)."
    }

    $installationPath = & $vswhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($installationPath)) {
        throw "MSVC C++ build tools are not installed."
    }

    $developerCommand = Join-Path $installationPath "Common7\Tools\VsDevCmd.bat"
    if (-not (Test-Path -LiteralPath $developerCommand -PathType Leaf)) {
        throw "Visual Studio developer environment was not found: $developerCommand"
    }

    $environmentCommand = "call `"$developerCommand`" -no_logo -arch=x64 -host_arch=x64 >nul && set"
    $environmentLines = & $env:ComSpec /d /s /c $environmentCommand
    if ($LASTEXITCODE -ne 0) {
        throw "Visual Studio developer environment initialization failed."
    }

    foreach ($line in $environmentLines) {
        $separator = $line.IndexOf("=")
        if ($separator -le 0) {
            continue
        }

        $name = $line.Substring(0, $separator)
        $value = $line.Substring($separator + 1)
        [Environment]::SetEnvironmentVariable($name, $value, "Process")
    }

    if ($null -eq (Get-Command link.exe -ErrorAction SilentlyContinue)) {
        throw "MSVC link.exe is unavailable after loading Visual Studio Build Tools."
    }

    if ([string]::IsNullOrWhiteSpace($env:WindowsSdkDir) -or
        -not (Test-Path -LiteralPath $env:WindowsSdkDir -PathType Container)) {
        throw "Windows SDK is unavailable after loading Visual Studio Build Tools."
    }
}

try {
    $npm = Resolve-Executable "npm.cmd"
    $cargo = Resolve-Executable "cargo.exe" @("$env:USERPROFILE\.cargo\bin\cargo.exe")
    $rustc = Resolve-Executable "rustc.exe" @("$env:USERPROFILE\.cargo\bin\rustc.exe")
    $rustup = Resolve-Executable "rustup.exe" @("$env:USERPROFILE\.cargo\bin\rustup.exe")
    $systemPython = Resolve-Executable "python.exe"

    Invoke-Stage "Windows Rust toolchain" {
        Import-MsvcEnvironment

        $rustDetails = & $rustc --version --verbose
        if ($LASTEXITCODE -ne 0) {
            throw "Unable to inspect the active Rust compiler."
        }
        $rustDetails | Write-Host
        if (($rustDetails -join "`n") -notmatch "host: x86_64-pc-windows-msvc") {
            throw "The active Rust compiler is not the x86_64-pc-windows-msvc toolchain."
        }

        Invoke-Native $cargo @("--version")

        $toolchains = & $rustup toolchain list
        if ($LASTEXITCODE -ne 0) {
            throw "Unable to inspect installed Rust toolchains."
        }
        $toolchains | Write-Host
        if (($toolchains -join "`n") -notmatch "stable-x86_64-pc-windows-msvc") {
            throw "The stable x86_64-pc-windows-msvc Rust toolchain is not installed."
        }

        $targets = & $rustup target list --installed
        if ($LASTEXITCODE -ne 0) {
            throw "Unable to inspect installed Rust targets."
        }
        $targets | Write-Host
        if ($targets -notcontains "x86_64-pc-windows-msvc") {
            throw "The x86_64-pc-windows-msvc Rust target is not installed."
        }

        $windowsSdkVersion = $env:WindowsSDKVersion.TrimEnd("\")
        $windowsSdkLib = Join-Path $env:WindowsSdkDir "Lib\$windowsSdkVersion"
        if (-not (Test-Path -LiteralPath $windowsSdkLib -PathType Container)) {
            throw "Windows SDK libraries were not found: $windowsSdkLib"
        }

        Write-Host "MSVC compiler: $(Resolve-Executable 'cl.exe')"
        Write-Host "MSVC linker: $(Resolve-Executable 'link.exe')"
        Write-Host "Windows SDK: $windowsSdkLib"
    }

    Invoke-Stage "TypeScript dependencies" {
        Invoke-Native $npm @("ci")
    }
    Invoke-Stage "TypeScript lint" {
        Invoke-Native $npm @("run", "lint")
    }
    Invoke-Stage "TypeScript format check" {
        Invoke-Native $npm @("run", "format:check")
    }
    Invoke-Stage "TypeScript typecheck" {
        Invoke-Native $npm @("run", "typecheck")
    }
    Invoke-Stage "TypeScript tests" {
        Invoke-Native $npm @("test")
    }
    Invoke-Stage "TypeScript production build" {
        Invoke-Native $npm @("run", "build")
    }

    Invoke-Stage "Rust format check" {
        Invoke-Native $cargo @("fmt", "--all", "--", "--check")
    }
    Invoke-Stage "Rust clippy" {
        Invoke-Native $cargo @(
            "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
        )
    }
    Invoke-Stage "Rust workspace build" {
        Invoke-Native $cargo @("build", "--workspace", "--all-features")
    }
    Invoke-Stage "Rust workspace tests" {
        Invoke-Native $cargo @("test", "--workspace", "--all-features")
    }
    Invoke-Stage "SQLite integration test" {
        Invoke-Native $cargo @(
            "test",
            "--package", "meeting-intelligence-integration-tests",
            "--test", "offline_pipeline",
            "sqlite_storage_round_trips_canonical_meeting",
            "--", "--exact"
        )
    }
    Invoke-Stage "Offline mock pipeline test" {
        Invoke-Native $cargo @(
            "test",
            "--package", "meeting-intelligence-integration-tests",
            "--test", "offline_pipeline",
            "fixture_pipeline_exports_markdown_and_json_without_guessing_speakers",
            "--", "--exact"
        )
    }
    Invoke-Stage "Offline mock pipeline executable" {
        $outputDirectory = Join-Path $repositoryRoot "target\local-validation-output"
        if (Test-Path -LiteralPath $outputDirectory) {
            Remove-Item -LiteralPath $outputDirectory -Recurse -Force
        }

        Invoke-Native $cargo @(
            "run", "--package", "meeting-intelligence-server", "--",
            "fixtures", $outputDirectory
        )

        foreach ($artifactName in @("meeting.md", "meeting.json")) {
            $artifactPath = Join-Path $outputDirectory $artifactName
            if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf) -or
                (Get-Item -LiteralPath $artifactPath).Length -eq 0) {
                throw "Offline mock pipeline artifact is missing or empty: $artifactPath"
            }
        }
    }

    Invoke-Stage "Python environment" {
        $venvPython = Join-Path $repositoryRoot ".venv\Scripts\python.exe"
        if (-not (Test-Path -LiteralPath $venvPython -PathType Leaf)) {
            Invoke-Native $systemPython @("-m", "venv", ".venv")
        }
        Invoke-Native $venvPython @(
            "-m", "pip", "install", "--disable-pip-version-check", "-e", "services/local-ai[dev]"
        )
        $script:Python = $venvPython
    }
    Invoke-Stage "Python Ruff lint" {
        Invoke-Native $script:Python @("-m", "ruff", "check", "services/local-ai")
    }
    Invoke-Stage "Python Ruff format check" {
        Invoke-Native $script:Python @("-m", "ruff", "format", "--check", "services/local-ai")
    }
    Invoke-Stage "Python tests" {
        Invoke-Native $script:Python @("-m", "pytest", "services/local-ai")
    }

    Write-Host ""
    Write-Host "LOCAL VALIDATION: PASS"
}
catch {
    Write-Error "LOCAL VALIDATION: FAIL ($script:CurrentStage): $($_.Exception.Message)"
    exit 1
}
