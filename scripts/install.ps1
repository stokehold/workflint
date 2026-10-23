<#
.SYNOPSIS
Installs workflint from a GitHub release of stokehold/workflint.

.DESCRIPTION
Downloads the archive for this OS and architecture with checksums.txt, refuses
an archive whose SHA-256 does not match, checks the archive's attestation with
`gh attestation verify` when gh is installed and logged in, and puts the
workflint binary into Dir. Prints the binary's path, and appends
`executable=<path>` to $env:GITHUB_OUTPUT when that is set.

Runs with Windows PowerShell 5.1 and PowerShell 7 on Windows, and with
PowerShell 7 on Linux and macOS. docs/releasing.md describes it.

WORKFLINT_DOWNLOAD_URL replaces the base URL
https://github.com/stokehold/workflint/releases/download, for a mirror. The
files come from <base URL>/v<version>/, and latest is read from the redirect of
the base URL with /download replaced by /latest.

.PARAMETER Version
A version such as 0.1.0 or v0.1.0, or latest.

.PARAMETER Dir
The directory for the binary, created if missing. Defaults to the current
directory.

.PARAMETER RequireAttestation
Fail when the attestation cannot be checked because gh is missing or not logged
in. The environment variable WORKFLINT_REQUIRE_ATTESTATION=1 does the same.

.EXAMPLE
irm https://github.com/stokehold/workflint/releases/latest/download/install.ps1 | iex

.EXAMPLE
& ([scriptblock]::Create((irm https://github.com/stokehold/workflint/releases/latest/download/install.ps1))) -Version latest -Dir .\bin
#>
[CmdletBinding()]
param(
    [string]$Version = '',
    [string]$Dir = '',
    [switch]$RequireAttestation
)

# The version to install when no -Version is given. The release workflow sets
# it to the release's version in the copy it attaches to a release.
$DefaultVersion = 'latest'

function Install-Workflint {
    param(
        [string]$Version,
        [string]$Dir,
        [bool]$RequireAttestation,
        [string]$DefaultVersion
    )

    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'

    $repo = 'stokehold/workflint'
    $signerWorkflow = "$repo/.github/workflows/release.yml"
    $semver = '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'

    function Write-Status([string]$Message) {
        [Console]::Error.WriteLine($Message)
    }

    if (-not $Version) { $Version = $DefaultVersion }
    if (-not $Dir) { $Dir = (Get-Location).ProviderPath }
    switch ("$env:WORKFLINT_REQUIRE_ATTESTATION") {
        '' { }
        '0' { }
        '1' { $RequireAttestation = $true }
        default { throw "WORKFLINT_REQUIRE_ATTESTATION is '$env:WORKFLINT_REQUIRE_ATTESTATION', not 0 or 1" }
    }

    $baseUrl = 'https://github.com/stokehold/workflint/releases/download'
    if ($env:WORKFLINT_DOWNLOAD_URL) { $baseUrl = $env:WORKFLINT_DOWNLOAD_URL }
    $baseUrl = $baseUrl.TrimEnd('/')

    if ($PSVersionTable.PSEdition -ne 'Core') {
        # Windows PowerShell 5.1 may not offer TLS 1.2, which GitHub requires.
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    }

    # The OS and architecture as they appear in the archive names.
    $onWindows = ($PSVersionTable.PSEdition -ne 'Core') -or $IsWindows
    if ($onWindows) {
        $os = 'windows'
        # A 32-bit or emulated x64 process on Windows on Arm sees AMD64 or x86
        # in PROCESSOR_ARCHITECTURE. PROCESSOR_ARCHITEW6432 has the OS's
        # architecture in a 32-bit process, and PROCESSOR_IDENTIFIER the CPU's
        # in every process.
        $machine = $env:PROCESSOR_ARCHITECTURE
        if ($env:PROCESSOR_ARCHITEW6432) { $machine = $env:PROCESSOR_ARCHITEW6432 }
        if ("$env:PROCESSOR_IDENTIFIER" -like 'ARM*') { $machine = 'ARM64' }
    }
    else {
        if ($IsMacOS) { $os = 'darwin' } else { $os = 'linux' }
        $machine = (& uname -m).Trim()
        if ($IsMacOS -and $machine -eq 'x86_64') {
            # A process under Rosetta 2 on Apple silicon sees x86_64.
            $translated = (& sysctl -n sysctl.proc_translated 2> $null)
            if ("$translated".Trim() -eq '1') { $machine = 'arm64' }
        }
    }
    switch ($machine) {
        { $_ -in 'x86_64', 'amd64', 'AMD64' } { $arch = 'amd64'; break }
        { $_ -in 'aarch64', 'arm64', 'ARM64' } { $arch = 'arm64'; break }
        default { throw "workflint has no build for the architecture $machine; see https://github.com/$repo/releases" }
    }

    if ($Version -eq 'latest') {
        # Reads the version from the redirect of the latest-release URL, so no
        # GitHub API call or rate limit applies.
        $latestUrl = ($baseUrl -replace '/download$', '') + '/latest'
        Write-Status "Finding the latest version from $latestUrl"
        $request = [System.Net.HttpWebRequest]::Create($latestUrl)
        $request.AllowAutoRedirect = $false
        $request.Method = 'HEAD'
        $location = ''
        try {
            $response = $request.GetResponse()
            $location = "$($response.Headers['Location'])"
            $response.Close()
        }
        catch {
            throw "could not reach $latestUrl to find the latest version: $($_.Exception.Message)"
        }
        $tag = ($location -split '/')[-1]
        if ($tag -notmatch '^v' -or $tag.Substring(1) -notmatch $semver) {
            throw "could not find the latest version: $latestUrl redirects to '$location', not to a release tag"
        }
        $Version = $tag.Substring(1)
    }
    else {
        if ($Version.StartsWith('v')) { $Version = $Version.Substring(1) }
        if ($Version -notmatch $semver) {
            throw "the version '$Version' is not a version such as 0.1.0 or v0.1.0, or latest"
        }
    }

    if ($os -eq 'windows') {
        $archive = "workflint_${Version}_${os}_${arch}.zip"
        $binary = 'workflint.exe'
    }
    else {
        $archive = "workflint_${Version}_${os}_${arch}.tar.gz"
        $binary = 'workflint'
    }
    $releaseUrl = "$baseUrl/v$Version"

    # Downloads a URL into a file, with retries. Fails on an HTTP error, and
    # does not retry a 404.
    function Get-Download([string]$Url, [string]$OutFile) {
        for ($attempt = 1; ; $attempt++) {
            try {
                Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing
                return
            }
            catch {
                $status = 0
                if ($_.Exception.Response) { $status = [int]$_.Exception.Response.StatusCode }
                if ($attempt -ge 3 -or $status -eq 404) { throw }
                Start-Sleep -Seconds 1
            }
        }
    }

    # Whether gh is installed and logged in.
    function Test-GhLogin {
        if (-not (Get-Command -Name gh -ErrorAction SilentlyContinue)) { return $false }
        # Windows PowerShell 5.1 turns redirected standard error of a native
        # command into a terminating error under 'Stop'.
        $ErrorActionPreference = 'Continue'
        & gh auth status *> $null
        return ($LASTEXITCODE -eq 0)
    }

    $tmp = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath ('workflint-install-' + [guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Write-Status "Downloading workflint $Version for $os/$arch from $releaseUrl"
        $checksums = Join-Path -Path $tmp -ChildPath 'checksums.txt'
        try { Get-Download -Url "$releaseUrl/checksums.txt" -OutFile $checksums }
        catch { throw "could not download $releaseUrl/checksums.txt; is $Version a release of workflint? $($_.Exception.Message)" }
        $archivePath = Join-Path -Path $tmp -ChildPath $archive
        try { Get-Download -Url "$releaseUrl/$archive" -OutFile $archivePath }
        catch { throw "could not download $releaseUrl/${archive}: $($_.Exception.Message)" }

        # checksums.txt has one `<SHA-256>  <file name>` line per asset.
        $expected = ''
        foreach ($line in Get-Content -LiteralPath $checksums) {
            $fields = $line.Trim() -split '\s+'
            if ($fields.Count -ge 2 -and ($fields[1] -eq $archive -or $fields[1] -eq "*$archive")) {
                $expected = $fields[0].ToLowerInvariant()
                break
            }
        }
        if (-not $expected) { throw "checksums.txt has no SHA-256 for $archive; nothing was installed" }
        $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            throw "the SHA-256 of $archive is $actual, but checksums.txt expects $expected; nothing was installed"
        }
        Write-Status "Checked the SHA-256 of ${archive}: $actual"

        if (Test-GhLogin) {
            & gh attestation verify $archivePath --repo $repo --signer-workflow $signerWorkflow | Out-Null
            if ($LASTEXITCODE -ne 0) { throw "the attestation of $archive does not check out; nothing was installed" }
            Write-Status "Checked the attestation of $archive"
        }
        elseif ($RequireAttestation) {
            throw 'the attestation check needs gh, installed and logged in, and -RequireAttestation or WORKFLINT_REQUIRE_ATTESTATION=1 requires it; nothing was installed'
        }
        else {
            Write-Status "Skipped the attestation check because gh is missing or not logged in. To check it, download $archive and run: gh attestation verify $archive --repo $repo --signer-workflow $signerWorkflow"
        }

        $unpacked = Join-Path -Path $tmp -ChildPath 'unpacked'
        New-Item -ItemType Directory -Path $unpacked | Out-Null
        if ($os -eq 'windows') {
            Expand-Archive -LiteralPath $archivePath -DestinationPath $unpacked
        }
        else {
            & tar -xzf $archivePath -C $unpacked $binary
            if ($LASTEXITCODE -ne 0) { throw "could not unpack $archive" }
        }
        $unpackedBinary = Join-Path -Path $unpacked -ChildPath $binary
        if (-not (Test-Path -LiteralPath $unpackedBinary -PathType Leaf)) { throw "$archive has no $binary" }

        New-Item -ItemType Directory -Path $Dir -Force | Out-Null
        $Dir = (Resolve-Path -LiteralPath $Dir).ProviderPath
        $executable = Join-Path -Path $Dir -ChildPath $binary
        Move-Item -LiteralPath $unpackedBinary -Destination $executable -Force

        Write-Status "Installed workflint $Version to $executable"
        if ($env:GITHUB_OUTPUT) {
            # AppendAllText writes UTF-8 without a byte order mark, which
            # Add-Content in Windows PowerShell 5.1 would add to a new file.
            [System.IO.File]::AppendAllText($env:GITHUB_OUTPUT, "executable=$executable`n")
        }
        Write-Output $executable
    }
    finally {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

try {
    Install-Workflint -Version $Version -Dir $Dir -RequireAttestation $RequireAttestation.IsPresent -DefaultVersion $DefaultVersion
}
catch {
    # Run with `irm ... | iex` or as a script block, the script has no path.
    # Exiting there would close the caller's PowerShell, so the error goes to
    # the caller instead.
    if (-not $PSCommandPath) { throw }
    [Console]::Error.WriteLine("error: $($_.Exception.Message)")
    exit 1
}
