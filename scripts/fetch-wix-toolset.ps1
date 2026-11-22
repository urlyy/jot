# 预取 Tauri 打包所需的 WiX 工具链
#
# 背景：`tauri build` 产 MSI 时会从 GitHub 下载 wix314-binaries.zip（约 40MB）。
# 网络不稳定时 Tauri 自带的全局超时会失败（`timeout: global`），安装包就出不来。
# 这个脚本按同样的地址把工具链放进 Tauri 期望的缓存目录，之后打包会直接复用。
#
# 用法：pwsh -File scripts/fetch-wix-toolset.ps1

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$version = 'wix3141rtm'
$fileName = 'wix314-binaries.zip'
$dest = Join-Path $env:LOCALAPPDATA 'tauri\WixTools314'

if ((Test-Path (Join-Path $dest 'candle.exe')) -and (Test-Path (Join-Path $dest 'light.exe'))) {
    Write-Host "WiX 工具链已就绪：$dest"
    exit 0
}

$github = "https://github.com/wixtoolset/wix3/releases/download/$version/$fileName"
$mirrors = @(
    "https://ghfast.top/$github",
    "https://gh-proxy.com/$github",
    $github
)

$tmp = Join-Path $env:TEMP $fileName
$downloaded = $false

foreach ($url in $mirrors) {
    try {
        Write-Host "尝试下载：$url"
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        Invoke-WebRequest -Uri $url -OutFile $tmp -TimeoutSec 300
        $sw.Stop()
        Write-Host ("下载完成：{0:N1} MB，用时 {1:N1} 秒" -f ((Get-Item $tmp).Length / 1MB), ($sw.ElapsedMilliseconds / 1000))
        $downloaded = $true
        break
    } catch {
        Write-Warning "失败：$($_.Exception.Message)"
    }
}

if (-not $downloaded) {
    Write-Error "所有镜像都下载失败，请检查网络或配置代理后重试。"
    exit 1
}

New-Item -ItemType Directory -Force -Path $dest | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory($tmp, $dest, $true)
Remove-Item -Force $tmp -ErrorAction SilentlyContinue

if (-not ((Test-Path (Join-Path $dest 'candle.exe')) -and (Test-Path (Join-Path $dest 'light.exe')))) {
    Write-Error "解压后没有找到 candle.exe / light.exe，目录可能不对：$dest"
    exit 1
}

Write-Host "WiX 工具链已安装：$dest"
Write-Host "现在可以运行 npm run tauri build -- --bundles msi 产出安装包。"
