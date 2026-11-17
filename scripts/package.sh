#!/usr/bin/env bash
# Build release artifacts: tar.gz, .deb, .pkg.tar.zst (Arch), .rpm and AppImage.
# Usage: package.sh VERSION BIN_DIR OUT_DIR
set -euo pipefail

VERSION="${1:?usage: package.sh VERSION BIN_DIR OUT_DIR}"
BIN_DIR="${2:?usage: package.sh VERSION BIN_DIR OUT_DIR}"
OUT_DIR="${3:?usage: package.sh VERSION BIN_DIR OUT_DIR}"
ARCH="x86_64"
PKGNAME="slimblade"
DESC="Userspace button and scroll manager for Kensington SlimBlade trackballs"
URL="https://github.com/ioloryn/slimblade"

# rpmbuild resolves _topdir as absolute; relative paths break %prep.
mkdir -p "$OUT_DIR"
BIN_DIR="$(realpath "$BIN_DIR")"
OUT_DIR="$(realpath "$OUT_DIR")"

# Create a filesystem tree that is used as the base for every package format.
make_pkgroot() {
  local root="$1"
  rm -rf "$root"
  mkdir -p "$root/usr/bin" \
    "$root/usr/share/applications" \
    "$root/etc/xdg/autostart" \
    "$root/usr/lib/systemd/user" \
    "$root/etc/udev/rules.d" \
    "$root/usr/share/doc/slimblade" \
    "$root/usr/share/icons/hicolor/64x64/apps"

  install -m 755 "$BIN_DIR/slimblade" "$root/usr/bin/slimblade"
  install -m 755 "$BIN_DIR/slimblade-gui" "$root/usr/bin/slimblade-gui"
  install -m 644 packaging/slimblade-gui.desktop "$root/usr/share/applications/"
  install -m 644 packaging/slimblade-gui-autostart.desktop "$root/etc/xdg/autostart/"
  install -m 644 packaging/slimblade.service "$root/usr/lib/systemd/user/"
  install -m 644 packaging/70-slimblade.rules "$root/etc/udev/rules.d/"
  install -m 644 icons/icon.png "$root/usr/share/icons/hicolor/64x64/apps/slimblade.png"
  install -m 644 README.md LICENSE "$root/usr/share/doc/slimblade/"
}

# ---------------------------------------------------------------- tar.gz ----
TARROOT="$OUT_DIR/.tarroot"
make_pkgroot "$TARROOT"
tar -czf "$OUT_DIR/slimblade-${VERSION}-${ARCH}-unknown-linux-gnu.tar.gz" -C "$TARROOT" .

# ------------------------------------------------------------------- .deb ----
DEBROOT="$OUT_DIR/.debroot"
make_pkgroot "$DEBROOT"
mkdir -p "$DEBROOT/DEBIAN"
cat > "$DEBROOT/DEBIAN/control" <<EOF
Package: slimblade
Version: $VERSION
Section: utils
Priority: optional
Architecture: amd64
Maintainer: ioloryn <noreply@github.com>
Homepage: $URL
Depends: libc6, libwebkit2gtk-4.1-0
Description: $DESC
 Userspace evdev/uinput remapper with a Material 3 Tauri settings GUI.
EOF
if command -v dpkg-deb >/dev/null 2>&1; then
  dpkg-deb --root-owner-group --build "$DEBROOT" \
    "$OUT_DIR/slimblade_${VERSION}_amd64.deb"
else
  echo "dpkg-deb not found — skipping .deb" >&2
fi

# ------------------------------------------------------------- Arch package --
ARCHROOT="$OUT_DIR/.archroot"
make_pkgroot "$ARCHROOT"
PKGSIZE=$(du -sb "$ARCHROOT" | awk '{print $1}')
cat > "$ARCHROOT/.PKGINFO" <<EOF
pkgname = $PKGNAME
pkgver = $VERSION-1
pkgdesc = $DESC
url = $URL
license = MIT
arch = $ARCH
depends = glibc
optdepends = webkit2gtk-4.1: for the graphical settings window
packager = ioloryn <noreply@github.com>
size = $PKGSIZE
EOF
if tar --zstd -cf /dev/null -C "$ARCHROOT" . 2>/dev/null; then
  tar --zstd -cf "$OUT_DIR/slimblade-${VERSION}-${ARCH}.pkg.tar.zst" \
    -C "$ARCHROOT" .
else
  echo "zstd support missing — skipping Arch package" >&2
fi

# -------------------------------------------------------------------- .rpm ---
if command -v rpmbuild >/dev/null 2>&1; then
RPMROOT="$OUT_DIR/.rpmstage/slimblade-$VERSION"
make_pkgroot "$RPMROOT"
mkdir -p "$OUT_DIR/.rpmbuild"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
tar -czf "$OUT_DIR/.rpmbuild/SOURCES/slimblade-$VERSION.tar.gz" \
  -C "$OUT_DIR/.rpmstage" "slimblade-$VERSION"
cat > "$OUT_DIR/.rpmbuild/SPECS/slimblade.spec" <<EOF
%global _build_id_links none
%global debug_package %{nil}
Name:           slimblade
Version:        $VERSION
Release:        1%{?dist}
Summary:        $DESC
License:        MIT
URL:            $URL
Source0:        slimblade-%{version}.tar.gz
Requires:       glibc
Requires:       webkit2gtk4.1

%description
Userspace evdev/uinput remapper for Kensington SlimBlade with a
Material 3 settings window built with Tauri.

%prep
%setup -q

%install
mkdir -p %{buildroot}
cp -a . %{buildroot}/

%files
/usr/bin/slimblade
/usr/bin/slimblade-gui
/usr/share/applications/slimblade-gui.desktop
/etc/xdg/autostart/slimblade-gui-autostart.desktop
/usr/lib/systemd/user/slimblade.service
/etc/udev/rules.d/70-slimblade.rules
/usr/share/icons/hicolor/64x64/apps/slimblade.png
/usr/share/doc/slimblade/README.md
/usr/share/doc/slimblade/LICENSE

%changelog
* $(date '+%a %b %d %Y') ioloryn <noreply@github.com> - $VERSION-1
- Release $VERSION
EOF
rpmbuild --define "_topdir $OUT_DIR/.rpmbuild" -bb \
  "$OUT_DIR/.rpmbuild/SPECS/slimblade.spec"
find "$OUT_DIR/.rpmbuild/RPMS" -name '*.rpm' \
  -exec cp {} "$OUT_DIR/slimblade-${VERSION}.x86_64.rpm" \;
else
  echo "rpmbuild not found — skipping .rpm" >&2
fi

# ---------------------------------------------------------------- AppImage --
if command -v "${APPIMAGETOOL:-appimagetool}" >/dev/null 2>&1 \
   || [[ -x "${APPIMAGETOOL:-}" ]]; then
  APPDIR="$OUT_DIR/.AppDir"
  make_pkgroot "$APPDIR"
  cp packaging/slimblade-gui.desktop "$APPDIR/slimblade.desktop"
  cp icons/icon.png "$APPDIR/slimblade.png"
  sed -i 's/^Icon=.*/Icon=slimblade/' "$APPDIR/slimblade.desktop"
  cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
export LD_LIBRARY_PATH="$HERE/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$HERE/usr/bin/slimblade-gui" "$@"
RUN
  chmod +x "$APPDIR/AppRun"
  "${APPIMAGETOOL:-appimagetool}" "$APPDIR" \
    "$OUT_DIR/SlimBlade-${VERSION}-${ARCH}.AppImage"
else
  echo "appimagetool not found — skipping AppImage" >&2
fi

# Clean up intermediate directories; keep only final artifacts.
find "$OUT_DIR" -mindepth 1 -maxdepth 1 -type d -exec rm -rf {} +

echo "Artifacts in $OUT_DIR:"
ls -lh "$OUT_DIR"
