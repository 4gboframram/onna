# Maintainer 4gboframram <asdasd@gmail.com>

pkgname="onna"
pkgver="0.2.0"
pkgrel=1
pkgdesc="Real-time video player for the terminal"
arch=("x86_64")
url="https://github.com/4gboframram/onna"
license=("LGPL")
depends=("gstreamer" "gst-plugins-good" "gst-plugins-base" "gst-libav")
makedepends=("cargo")

prepare() {
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}
build() {
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release --all-features
}

check() {
    export RUSTUP_TOOLCHAIN=stable
    cargo test --workspace --frozen --all-features
}

package() {
    install -Dm0755 -t "$pkgdir/usr/bin/" "target/release/onna"
}