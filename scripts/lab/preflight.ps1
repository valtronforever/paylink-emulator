param(
    [Parameter(Mandatory=$true)][string]$PayLinkExe,
    [Parameter(Mandatory=$true)][string]$Installer,
    [Parameter(Mandatory=$true)][string]$SsiSpecification,
    [string]$Output = 'runs/preflight.json',
    [switch]$Running
)
$ErrorActionPreference = 'Stop'
$expected = @{
    installer = '62d0e7a539380937ecb5af2f1c50438e4b84a070de4a20c1c848c11a5e395491'
    executable = '85d1bf9280e7c63e1099ecc2db480813f439e68c41a5eca8a453094a948ab465'
    specification = '8e4b459ade28f4ee301de7751659d4bf365f9c0f2ac3508a6745a597158e1a65'
}
$paths = @{installer=$Installer; executable=$PayLinkExe; specification=$SsiSpecification}
$hashes = @{}
foreach ($key in $paths.Keys) {
    $hashes[$key] = (Get-FileHash -LiteralPath $paths[$key] -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hashes[$key] -ne $expected[$key]) { throw "$key hash mismatch; do not replace pinned hashes silently" }
}
$exePath = (Resolve-Path -LiteralPath $PayLinkExe).Path
$processes = @(Get-CimInstance Win32_Process -Filter "Name='POSServer.exe'" | Where-Object ExecutablePath -eq $exePath)
if ($Running -and $processes.Count -ne 1) { throw 'Expected exactly one POSServer from the verified executable path' }
$ports = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -In 3000,13001,13010,13011 | Select-Object LocalAddress,LocalPort,OwningProcess)
if (!$Running -and $ports.Count) { throw "Lab ports already occupied: $($ports | ConvertTo-Json -Compress)" }
$assembly = [System.Reflection.AssemblyName]::GetAssemblyName($exePath).Version.ToString()
if ($assembly -ne '2.1.20.10') { throw "Unexpected assembly $assembly" }
$nativeConfigPath = Join-Path (Split-Path $exePath) 'config/server.json'
$nativeConfig = if (Test-Path -LiteralPath $nativeConfigPath) { Get-Content -LiteralPath $nativeConfigPath -Raw | ConvertFrom-Json } else { $null }
$report = @{
    utc=[DateTime]::UtcNow.ToString('o'); hashes=$hashes; assembly=$assembly
    windows=[Environment]::OSVersion.VersionString; architecture=$env:PROCESSOR_ARCHITECTURE
    processes=@($processes | Select-Object ProcessId,ExecutablePath,CreationDate)
    ports=$ports; node=(& node --version); commit=(& git rev-parse HEAD)
    native_config_path=$nativeConfigPath
    native_settings=if ($nativeConfig) { @{server=$nativeConfig.server;port=$nativeConfig.port;tcp_port=$nativeConfig.TcpSocketPort;update_on_start=$nativeConfig.updateOnStartApplication;update_frequency=$nativeConfig.check_updates_frequency} } else { $null }
    ms_vc_available=[bool](Get-Command link.exe -ErrorAction SilentlyContinue)
}
$parent = Split-Path -Parent $Output
if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Output -Encoding utf8
Write-Output "Preflight saved to $Output"
