# 打版本标签并触发 GitHub Actions 发布
#
# 用法：
#   pwsh -File scripts/release.ps1              # 创建标签并推送，触发发布
#   pwsh -File scripts/release.ps1 -SkipPush    # 只在本地打标签（预览用）
#
# 前提：版本号先改好（src-tauri/tauri.conf.json、package.json 和 Cargo.toml 保持一致），
# 且工作区没有未提交的改动。

param(
    [switch]$SkipPush,
    [string]$Remote = 'origin'
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $version = (Get-Content 'src-tauri\tauri.conf.json' -Raw | ConvertFrom-Json).version
    $packageVersion = (Get-Content 'package.json' -Raw | ConvertFrom-Json).version

    $cargoVersion = (Select-String -Path 'src-tauri/Cargo.toml' -Pattern '^version = "([^"]+)"$').Matches.Groups[1].Value
    if ($version -ne $packageVersion -or $version -ne $cargoVersion) {
        throw "版本号不一致：tauri.conf.json=$version，package.json=$packageVersion，Cargo.toml=$cargoVersion"
    }

    $tags = git tag -l "v$version"
    if ($tags) {
        throw "标签 v$version 已存在。要重新发版请先提升版本号。"
    }

    $dirty = git status --porcelain
    if ($dirty) {
        throw "工作区有未提交的改动，先提交再发版。"
    }

    $tag = "v$version"
    git tag -a $tag -m "Jot $tag"
    if ($LASTEXITCODE -ne 0) { throw "创建标签失败" }

    if ($SkipPush) {
        Write-Host "已在本地创建标签 $tag（未推送）。"
        Write-Host "确认无误后运行：git push $Remote $tag"
        exit 0
    }

    git push $Remote HEAD
    if ($LASTEXITCODE -ne 0) { throw "推送分支失败" }

    git push $Remote $tag
    if ($LASTEXITCODE -ne 0) { throw "推送标签失败" }

    $url = (git remote get-url $Remote) -replace '\.git$', '' -replace '^git@github\.com:', 'https://github.com/'
    Write-Host "已推送标签 $tag。"
    Write-Host "发布进度：$url/actions"
    Write-Host "Release 页面：$url/releases/tag/$tag"
} finally {
    Pop-Location
}
