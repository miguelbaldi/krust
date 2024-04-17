#!/bin/bash
set -euo pipefail

dnf -y install mingw64-openssl-static mingw64-gcc-c++ zstd cyrus-sasl-devel perl
curl --connect-timeout 60 -m 60 -L -o /tmp/gtksourceview-5.pkg.tar.zst https://repo.msys2.org/mingw/mingw64/mingw-w64-x86_64-gtksourceview5-5.12.0-1-any.pkg.tar.zst
curl --connect-timeout 60 -m 60 -L -o /tmp/cyrus-sasl-2.pkg.tar.zst https://repo.msys2.org/mingw/mingw64/mingw-w64-x86_64-cyrus-sasl-2.1.28-3-any.pkg.tar.zst
curl --connect-timeout 60 -m 60 -L -o /tmp/libepoxy-1.5.10-5.pkg.tar.zst https://repo.msys2.org/mingw/mingw64/mingw-w64-x86_64-libepoxy-1.5.10-5-any.pkg.tar.zst
cd /tmp
tar --use-compress-program=unzstd -xvf gtksourceview-5.pkg.tar.zst
tar --use-compress-program=unzstd -xvf cyrus-sasl-2.pkg.tar.zst
tar --use-compress-program=unzstd -xvf libepoxy-1.5.10-5.pkg.tar.zst

# hotfix libepoxy (Windows 11 Access Memory Violation - segmentation fault)
cp -fvr /tmp/mingw64/bin/libepoxy-0.dll /usr/x86_64-w64-mingw32/sys-root/mingw/bin/libepoxy-0.dll
# libsasl-2
cp -fvr /tmp/mingw64/sbin/saslpasswd2.exe /usr/x86_64-w64-mingw32/sys-root/mingw/sbin/saslpasswd2.exe
cp -fvr /tmp/mingw64/sbin/sasldblistusers2.exe /usr/x86_64-w64-mingw32/sys-root/mingw/sbin/sasldblistusers2.exe
cp -fvr /tmp/mingw64/sbin/pluginviewer.exe /usr/x86_64-w64-mingw32/sys-root/mingw/sbin/pluginviewer.exe
cp -fvr /tmp/mingw64/bin/libsasl2-3.dll /usr/x86_64-w64-mingw32/sys-root/mingw/bin/libsasl2-3.dll
cp -fvr /tmp/mingw64/include/sasl/ /usr/x86_64-w64-mingw32/sys-root/mingw/include/
cp -fvr /tmp/mingw64/lib/libsasl2.dll.a /usr/x86_64-w64-mingw32/sys-root/mingw/lib/
cp -fvr /tmp/mingw64/lib/pkgconfig/libsasl2.pc /usr/x86_64-w64-mingw32/sys-root/mingw/lib/pkgconfig/
cp -fvr /tmp/mingw64/lib/sasl2/ /usr/x86_64-w64-mingw32/sys-root/mingw/lib/

# GtkSourceview-5
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
build && package
