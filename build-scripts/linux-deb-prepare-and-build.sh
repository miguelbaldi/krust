#!/bin/bash
# Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
# this source code is governed by the GPL-3.0 license that can be
# found in the COPYING file.

set -euo pipefail

# install system dependencies
apt-get update && apt-get install -y pkg-config libgtk-4-dev libadwaita-1-dev libsasl2-dev libgtksourceview-5-dev openssl curl gcc git build-essential cmake
pkgconf --modversion libadwaita-1
# cargo install cargo-rpm && cargo rpm build
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
cd /mnt
git config --global --add safe.directory /mnt
source ~/.cargo/env
cargo install cargo-deb
cargo build --release
source version.sh
cargo deb --deb-version "$GIT_TAG_LATEST"
