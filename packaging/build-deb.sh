#!/bin/sh
# Build a native Debian package from the workspace root.
set -eu
umask 022
export LC_ALL=C TZ=UTC

root=$(pwd -P)
target=${CARGO_TARGET_DIR:-target}
mkdir -p "$target/deb"
target=$(cd "$target" && pwd -P)
out=$target/deb
work=$(mktemp -d "$out/.snag.XXXXXXXX")
trap 'rm -rf -- "$work"' 0
trap 'exit 1' HUP INT TERM
pkg=$work/package

: "${SOURCE_DATE_EPOCH:=$(git log -1 --format=%ct)}"
export SOURCE_DATE_EPOCH
host=$(rustc -vV | sed -n 's/^host: //p')
sysroot=$(rustc --print sysroot)
cargo_home=$(cd "${CARGO_HOME:-$HOME/.cargo}" && pwd -P)
arch=$(dpkg --print-architecture)
# Cargo accepts ASCII unit separators between flags, including paths with spaces.
sep=$(printf '\037')
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$root=/src/snag"
for map in "$cargo_home=/cargo" "$sysroot=/rust" "$target=/target"; do
    CARGO_ENCODED_RUSTFLAGS="$CARGO_ENCODED_RUSTFLAGS${sep}--remap-path-prefix=$map"
done
export CARGO_ENCODED_RUSTFLAGS
cargo build --locked --release --target "$host" --target-dir "$target" -p snag-app

install -d "$pkg/DEBIAN" "$work/debian"
install -Dm755 "$target/$host/release/snag" "$pkg/usr/bin/snag"
install -Dm644 packaging/snag.desktop "$pkg/usr/share/applications/snag.desktop"
install -Dm644 packaging/snag.svg "$pkg/usr/share/icons/hicolor/scalable/apps/snag.svg"

# Use Cargo's resolved source paths so vendored crates and registry mirrors work.
cargo tree --locked --target "$host" -p snag-app -e normal --prefix none > "$work/tree"
cargo metadata --locked --format-version 1 --filter-platform "$host" > "$work/metadata"
# JSON::PP is part of the Perl installation required by dpkg-dev.
perl -MJSON::PP -e '
    use strict;
    use warnings;
    my ($tree, $metadata) = @ARGV;
    open my $list, "<", $tree or die "$tree: $!";
    my %used;
    while (<$list>) {
        my ($name, $version) = /^(\S+) v(\S+)/ or die "unexpected Cargo tree entry: $_";
        $used{"$name $version"} = 1;
    }
    open my $json, "<", $metadata or die "$metadata: $!";
    my $packages = decode_json(do { local $/; <$json> })->{packages};
    for my $p (@$packages) {
        next unless $p->{source} && $used{"$p->{name} $p->{version}"};
        print join("\t", $p->{name}, $p->{version}, $p->{manifest_path},
            $p->{license} // $p->{license_file}), "\n";
    }
' "$work/tree" "$work/metadata" > "$work/crates"
sort -u "$work/crates" -o "$work/crates"
version=$(awk '$1 == "snag-app" {print substr($2,2); exit}' "$work/tree")
test -n "$version"

copyright=$pkg/usr/share/doc/snag/copyright
install -d "$(dirname "$copyright")"
{
    printf 'Upstream: https://github.com/AndreiIlie/snag\n\n'
    cat LICENSE
    while IFS="$(printf '\t')" read -r name ver manifest license; do
        printf '\n%s %s (%s)\n' "$name" "$ver" "$license"
        dir=${manifest%/*}
        # Some crates carry additional notices in subdirectories.
        (cd "$dir" && find . -type f \( -iname '*license*' -o -iname 'copying*' \
            -o -iname 'copyright*' -o -iname 'notice*' \) > "$work/files")
        sort "$work/files" -o "$work/files"
        while IFS= read -r file; do
            printf '\n%s\n' "$file"
            cat "$dir/$file"
        done < "$work/files"
    done < "$work/crates"
    printf '\nRust standard library (%s, MIT OR Apache-2.0)\n' "$(rustc --version)"
    rust_doc=$sysroot/share/doc/rust
    if [ -f "$rust_doc/COPYRIGHT-library.html" ]; then
        cat "$rust_doc/COPYRIGHT-library.html"
    elif [ -f "$rust_doc/COPYRIGHT" ]; then
        cat "$rust_doc/COPYRIGHT"
    elif [ -f "$rust_doc/COPYRIGHT.html" ]; then
        cat "$rust_doc/COPYRIGHT.html"
    else
        # Distribution toolchains install notices alongside the standard library package.
        libdir=$(rustc --print target-libdir)
        owner=$(dpkg-query -S "$libdir/libstd-*.rlib")
        cat "/usr/share/doc/${owner%%:*}/copyright"
    fi
} > "$copyright"

# dpkg-shlibdeps reads source package metadata from debian/control.
cat > "$work/debian/control" <<CONTROL
Source: snag
Section: graphics
Priority: optional
Maintainer: Andrei Ilie <8228685+AndreiIlie@users.noreply.github.com>

Package: snag
Architecture: any
Description: Screenshot, annotation and clipboard tool
CONTROL
substvars=$(cd "$work" && dpkg-shlibdeps -O -e"$pkg/usr/bin/snag")
shared=$(printf '%s\n' "$substvars" | sed -n 's/^shlibs:Depends=//p')
test -n "$shared"
cat > "$pkg/DEBIAN/control" <<CONTROL
Package: snag
Version: $version
Architecture: $arch
Maintainer: Andrei Ilie <8228685+AndreiIlie@users.noreply.github.com>
Section: graphics
Priority: optional
Homepage: https://github.com/AndreiIlie/snag
Depends: $shared, libwayland-client0, libxkbcommon0, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxi6, libxkbcommon-x11-0, xdg-desktop-portal
Recommends: xdg-desktop-portal-gnome | xdg-desktop-portal-kde | xdg-desktop-portal-gtk, hicolor-icon-theme
Description: Screenshot, annotation and clipboard tool
 Capture and annotate screenshots on Linux, then save or copy them as PNG.
CONTROL
find "$pkg" -exec touch -d "@$SOURCE_DATE_EPOCH" {} +
deb=snag_${version}_${arch}.deb
dpkg-deb --root-owner-group --uniform-compression --threads-max=1 -Zxz -z6 -Snone \
    --build "$pkg" "$work/$deb" >&2
mv -f "$work/$deb" "$out/$deb"
printf '%s\n' "$out/$deb"
