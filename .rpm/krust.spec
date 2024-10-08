%define __spec_install_post %{nil}
%define __os_install_post %{_dbpath}/brp-compress
%define debug_package %{nil}

Name: krust
Summary: Kafka desktop client
Version: @@VERSION@@
Release: @@RELEASE@@%{?dist}
License: GPL-3.0-or-later
Group: Development/Tools
Source0: %{name}-%{version}.tar.gz

BuildRoot: %{_tmppath}/%{name}-%{version}-%{release}-root

Requires: gtksourceview5
Requires: libadwaita
Requires: cyrus-sasl-devel
Requires: openssl-devel

%description
%{summary}

%prep
%setup -q

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a * %{buildroot}
mkdir -p %{buildroot}/usr/share/applications
mkdir -p %{buildroot}/usr/share/pixmaps
cp -a ../../../../../data/images/io.miguelbaldi.KRust* %{buildroot}/usr/share/pixmaps/
cp -a ../../../../../data/images/krust.png %{buildroot}/usr/share/pixmaps/
cp -a ../../../../../data/images/krust.svg %{buildroot}/usr/share/pixmaps/
cp -a ../../../../../*.desktop %{buildroot}/usr/share/applications/

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%{_bindir}/*
%{_datadir}/applications/%{name}.desktop
%{_datadir}/pixmaps/%{name}.svg
%{_datadir}/pixmaps/%{name}.png
%{_datadir}/pixmaps/io.miguelbaldi.KRust.png
%{_datadir}/pixmaps/io.miguelbaldi.KRust.svg
%{_datadir}/pixmaps/io.miguelbaldi.KRust-symbolic.svg
%{_datadir}/pixmaps/io.miguelbaldi.KRust-symbolic.png
