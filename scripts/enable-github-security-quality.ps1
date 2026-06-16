param(
    [string]$Repository = "",
    [switch]$DryRun,
    [switch]$SkipCodeQL
)

$ErrorActionPreference = "Stop"

trap {
    Write-Host ""
    Write-Host "[ERROR] $($_.Exception.Message)"
    exit 1
}

function Resolve-GitHubCli {
    $command = Get-Command gh -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $defaultPath = "C:\Program Files\GitHub CLI\gh.exe"
    if (Test-Path -LiteralPath $defaultPath) {
        return $defaultPath
    }

    throw "GitHub CLI was not found. Install it with: winget install --id GitHub.cli --exact"
}

function Get-RepositoryFromRemote {
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $remote = git remote get-url origin 2>$null
    $gitExitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousErrorActionPreference

    if ($gitExitCode -ne 0 -or [string]::IsNullOrWhiteSpace($remote)) {
        throw "No repository was provided and no git origin remote was found."
    }

    if ($remote -match "github\.com[:/](?<owner>[^/]+)/(?<repo>[^/.]+)(\.git)?$") {
        return "$($Matches.owner)/$($Matches.repo)"
    }

    throw "Could not parse GitHub owner and repo from origin remote: $remote"
}

function Invoke-Gh {
    param(
        [string[]]$Arguments,
        [string]$InputJson = "",
        [switch]$AllowFailure
    )

    $printable = "gh " + ($Arguments -join " ")
    if ($DryRun) {
        Write-Host "[DRY RUN] $printable"
        if (-not [string]::IsNullOrWhiteSpace($InputJson)) {
            Write-Host $InputJson
        }
        return ""
    }

    if ([string]::IsNullOrWhiteSpace($InputJson)) {
        $output = & $script:GhPath @Arguments 2>&1
    } else {
        $output = $InputJson | & $script:GhPath @Arguments 2>&1
    }

    if ($LASTEXITCODE -ne 0) {
        $message = ($output | Out-String).Trim()
        if ($AllowFailure) {
            Write-Warning "$printable failed: $message"
            return $message
        }
        throw "$printable failed: $message"
    }

    return ($output | Out-String).Trim()
}

function Invoke-GhStep {
    param(
        [string]$Name,
        [string[]]$Arguments,
        [string]$InputJson = "",
        [switch]$AllowFailure
    )

    Write-Host "[INFO] $Name"
    return Invoke-Gh -Arguments $Arguments -InputJson $InputJson -AllowFailure:$AllowFailure
}

$script:GhPath = Resolve-GitHubCli

if ([string]::IsNullOrWhiteSpace($Repository)) {
    $Repository = Get-RepositoryFromRemote
}

if ($Repository -notmatch "^[^/\s]+/[^/\s]+$") {
    throw "Repository must use owner/repo format."
}

Write-Host "============================================================"
Write-Host "  Enable GitHub Security And Quality Features"
Write-Host "============================================================"
Write-Host ""
Write-Host "[INFO] Repository: $Repository"
Write-Host "[INFO] GitHub CLI: $script:GhPath"
if ($DryRun) {
    Write-Host "[INFO] Dry run: enabled"
}
Write-Host ""

Invoke-GhStep -Name "Checking GitHub authentication" -Arguments @("auth", "status") | Out-Null

$repoInfo = Invoke-GhStep `
    -Name "Checking repository access" `
    -Arguments @("repo", "view", $Repository, "--json", "nameWithOwner,visibility,isPrivate,viewerPermission")

Write-Host $repoInfo

Invoke-GhStep `
    -Name "Enabling Dependabot alerts" `
    -Arguments @("api", "-X", "PUT", "repos/$Repository/vulnerability-alerts", "--silent") | Out-Null

Start-Sleep -Seconds 2

Invoke-GhStep `
    -Name "Enabling Dependabot security updates" `
    -Arguments @("api", "-X", "PUT", "repos/$Repository/automated-security-fixes", "--silent") | Out-Null

Invoke-GhStep `
    -Name "Enabling private vulnerability reporting" `
    -Arguments @("api", "-X", "PUT", "repos/$Repository/private-vulnerability-reporting", "--silent") `
    -AllowFailure | Out-Null

$securityBody = @{
    security_and_analysis = @{
        secret_scanning = @{
            status = "enabled"
        }
        secret_scanning_push_protection = @{
            status = "enabled"
        }
        secret_scanning_non_provider_patterns = @{
            status = "enabled"
        }
        secret_scanning_validity_checks = @{
            status = "enabled"
        }
    }
} | ConvertTo-Json -Depth 10

Invoke-GhStep `
    -Name "Enabling secret scanning options" `
    -Arguments @("api", "-X", "PATCH", "repos/$Repository", "--input", "-", "--jq", "{security_and_analysis}") `
    -InputJson $securityBody `
    -AllowFailure | Out-Null

if (-not $SkipCodeQL) {
    $codeQlBody = @{
        state = "configured"
        query_suite = "extended"
        threat_model = "remote_and_local"
        runner_type = "standard"
    } | ConvertTo-Json -Depth 5

    $codeQlResult = Invoke-GhStep `
        -Name "Enabling CodeQL default setup" `
        -Arguments @("api", "-X", "PATCH", "repos/$Repository/code-scanning/default-setup", "--input", "-", "--jq", ".") `
        -InputJson $codeQlBody `
        -AllowFailure

    if (-not [string]::IsNullOrWhiteSpace($codeQlResult)) {
        Write-Host $codeQlResult
    }
}

Write-Host ""
Write-Host "[INFO] Final repository security state"
Invoke-Gh `
    -Arguments @("api", "repos/$Repository", "--jq", "{full_name,visibility,security_and_analysis}") |
    Write-Host

Write-Host ""
Write-Host "[INFO] Dependabot alerts status"
Invoke-Gh `
    -Arguments @("api", "-X", "GET", "repos/$Repository/vulnerability-alerts", "--include") |
    Select-String -Pattern "HTTP/" |
    ForEach-Object { Write-Host $_.Line }

Write-Host ""
Write-Host "[INFO] Private vulnerability reporting status"
Invoke-Gh `
    -Arguments @("api", "repos/$Repository/private-vulnerability-reporting", "--jq", ".") `
    -AllowFailure |
    Write-Host

if (-not $SkipCodeQL) {
    Write-Host ""
    Write-Host "[INFO] CodeQL default setup status"
    Invoke-Gh `
        -Arguments @("api", "repos/$Repository/code-scanning/default-setup", "--jq", ".") `
        -AllowFailure |
        Write-Host
}

Write-Host ""
Write-Host "[INFO] Done."
