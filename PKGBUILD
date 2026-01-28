pkgname=napm
pkgver=0.1.2
pkgrel=1
pkgdesc="NeoArch package manager"
arch=('x86_64')
url="https://github.com/neoarchlinux/napm"
license=('GPL-3.0')

depends=(
    acl brotli bzip2 curl e2fsprogs gcc-libs glibc gpgme keyutils krb5
    libarchive libassuan libb2 libgpg-error libidn2 libnghttp2 libnghttp3
    libpsl libssh2 libunistring libxml2 lz4 openssl pacman xz zlib zstd
)

makedepends=(rust cargo clang pkgconf)

source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

binary='napm'
target='x86_64-unknown-linux-gnu'

build() {
    cd "$pkgname-$pkgver"
    cargo build --release --locked --target $target
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 target/$target/release/$binary "$pkgdir/usr/bin/$binary"
}
