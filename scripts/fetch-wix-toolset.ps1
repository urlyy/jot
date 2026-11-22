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
$dest = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'tauri\WixTools314'
# 与 Tauri CLI 2.11.2 的 WiX 校验值和必需文件一致。
$sha256 = '6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31'
$requiredFiles = @(
    'candle.exe', 'candle.exe.config', 'darice.cub', 'light.exe', 'light.exe.config',
    'wconsole.dll', 'winterop.dll', 'wix.dll', 'WixUIExtension.dll', 'WixUtilExtension.dll'
)

function Test-WixCache {
    foreach ($file in $requiredFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $dest $file) -PathType Leaf)) {
            return $false
        }
    }
    return $true
}

if (Test-WixCache) {
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
        if ((Get-FileHash -LiteralPath $tmp -Algorithm SHA256).Hash -ne $sha256) {
            throw 'WiX 压缩包 SHA-256 校验失败。'
        }
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

if (-not (Test-WixCache)) {
    Write-Error "WiX 缓存缺少打包所需文件：$dest"
    exit 1
}

Write-Host "WiX 工具链已安装：$dest"
Write-Host "现在可以运行 npm run tauri build -- --bundles msi 产出安装包。"
