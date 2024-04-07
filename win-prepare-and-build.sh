#!/bin/bash
set -euo pipefail

dnf -y install mingw64-gcc-c++ zstd
curl --connect-timeout 60 -m 60 -L -o /tmp/gtksourceview-5.pkg.tar.zst https://mirror.msys2.org/mingw/mingw64/mingw-w64-x86_64-gtksourceview5-5.12.0-1-any.pkg.tar.zst
cd /tmp
tar --use-compress-program=unzstd -xvf gtksourceview-5.pkg.tar.zst

cp -fvr /tmp/mingw64/bin/libgtksourceview-5-0.dll /usr/x86_64-w64-mingw32/sys-root/mingw/bin/libgtksourceview-5-0.dll
cp -fvr /tmp/mingw64/include/gtksourceview-5/ /usr/x86_64-w64-mingw32/sys-root/mingw/include/
cp -fvr /tmp/mingw64/share/gtksourceview-5/ /usr/x86_64-w64-mingw32/sys-root/mingw/share/
cp -fvr /tmp/mingw64/share/gir-1.0/ /usr/x86_64-w64-mingw32/sys-root/mingw/share/
cp -fvr /tmp/mingw64/share/icons/hicolor/scalable/actions/ /usr/x86_64-w64-mingw32/sys-root/mingw/share/icons/hicolor/scalable/
cp -fvr /tmp/mingw64/lib/libgtksourceview-5.a /usr/x86_64-w64-mingw32/sys-root/mingw/lib/
cp -fvr /tmp/mingw64/lib/libgtksourceview-5.dll.a /usr/x86_64-w64-mingw32/sys-root/mingw/lib/
cp -fvr /tmp/mingw64/lib/pkgconfig/gtksourceview-5.pc /usr/x86_64-w64-mingw32/sys-root/mingw/lib/pkgconfig/
cp -fvr /tmp/mingw64/lib/girepository-1.0/ /usr/x86_64-w64-mingw32/sys-root/mingw/lib/

cd /mnt
mkdir -p package/share/icons/hicolor/scalable/
cp -rfv /tmp/mingw64/share/icons/hicolor/scalable/actions package/share/icons/hicolor/scalable/
cp -rfv /tmp/mingw64/share/gtksourceview-5 package/share/
cp -rfv /tmp/mingw64/lib/girepository-1.0/ package/lib/
cp -rfv /tmp/mingw64/share/gir-1.0/ package/share/
export CHRONO_TZ_TIMEZONE_FILTER="(GMT|UTC|Brazil/.*)"
build --release && package
