# Maintainer: 乌龙 <yuanjingINT@users.noreply.github.com>
# Based on Miyu by SHORiN-KiWATA

pkgname=laozhou
pkgver=26.1.0
pkgrel=1
pkgdesc="终端里的运维老油条 — 基于 Miyu 框架改造的 Linux AI 助手"
arch=('x86_64')
url="https://github.com/yuanjingINT/Laozhou"
license=('MIT')
depends=('gcc-libs' 'openssl' 'chafa')
makedepends=('cargo' 'git')
source=()
sha256sums=()

prepare() {
    mkdir -p "$srcdir/Laozhou"
    cp -r /home/yuanjing/Documents/vibe\ codeing/laozhou/laozhou/* "$srcdir/Laozhou/"
    cp -r /home/yuanjing/Documents/vibe\ codeing/laozhou/laozhou/.git "$srcdir/Laozhou/" 2>/dev/null || true
}

build() {
    cd "$srcdir/Laozhou"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release
}

package() {
    cd "$srcdir/Laozhou"
    install -Dm755 "target/release/laozhou" "$pkgdir/usr/bin/laozhou"

    # 安装默认知识库
    install -d "$pkgdir/usr/share/laozhou/default-kb"
    cp -r kb/* "$pkgdir/usr/share/laozhou/default-kb/"

    # 安装默认提示词
    install -Dm644 "src/prompts/laozhou.md" "$pkgdir/usr/share/laozhou/prompts/laozhou.md"

    # 安装表情包
    install -d "$pkgdir/usr/share/laozhou/memes"
    cp -r src/memes/laozhou/* "$pkgdir/usr/share/laozhou/memes/" 2>/dev/null || true

    # 安装 LICENSE
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
