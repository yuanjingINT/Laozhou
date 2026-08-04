# Maintainer: 乌龙 <yuanjingINT@users.noreply.github.com>
# Based on Miyu by SHORiN-KiWATA

pkgname=laozhou
pkgver=26.1.3
pkgrel=1
pkgdesc="终端里的运维老油条 — 基于 Miyu 框架改造的 Linux AI 助手"
arch=('x86_64')
url="https://github.com/yuanjingINT/Laozhou"
license=('MIT')
depends=('gcc-libs' 'openssl' 'chafa')
makedepends=()
source=()
sha256sums=()

# 使用本地预编译二进制和项目资源打包
_srcdir="/home/yuanjing/Documents/vibe codeing/laozhou/laozhou"

package() {
    # 安装预编译二进制
    install -Dm755 "$_srcdir/target/release/laozhou" "$pkgdir/usr/bin/laozhou"

    # 安装默认知识库
    install -d "$pkgdir/usr/share/laozhou/default-kb"
    cp -r "$_srcdir"/kb/* "$pkgdir/usr/share/laozhou/default-kb/"

    # 安装默认提示词
    install -Dm644 "$_srcdir/src/prompts/laozhou.md" "$pkgdir/usr/share/laozhou/prompts/laozhou.md"

    # 安装示例人格（10 个，由 persona_generator 生成）
    install -d "$pkgdir/usr/share/laozhou/personas"
    cp "$_srcdir"/personas/*.md "$pkgdir/usr/share/laozhou/personas/" 2>/dev/null || true

    # 安装示例用户身份（linux小白 / 老板 / 默认）
    install -d "$pkgdir/usr/share/laozhou/identities"
    cp "$_srcdir"/identities/*.md "$pkgdir/usr/share/laozhou/identities/" 2>/dev/null || true

    # 安装表情包
    install -d "$pkgdir/usr/share/laozhou/memes"
    cp -r "$_srcdir"/src/memes/laozhou/* "$pkgdir/usr/share/laozhou/memes/" 2>/dev/null || true

    # 安装人设生成器
    install -Dm755 "$_srcdir/persona_generator.py" "$pkgdir/usr/share/laozhou/persona_generator.py"

    # 安装 LICENSE
    install -Dm644 "$_srcdir/LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
