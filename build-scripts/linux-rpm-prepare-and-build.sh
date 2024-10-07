#!/bin/bash
# Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
# this source code is governed by the GPL-3.0 license that can be
# found in the COPYING file.

set -euo pipefail

#dnf install -y strace rpm-build

# cargo install cargo-rpm && cargo rpm build
cargo install cargo-generate-rpm
cargo build --release
cargo generate-rpm --set-metadata="version = '$(git describe)'"
cargo appimage
