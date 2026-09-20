// Copyright (C) 2021-2026 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::env;

use grev::git_revision;


fn main() {
  let manifest_dir =
    env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR variable not set");
  let pkg_version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION variable not set");

  let git_rev = git_revision(manifest_dir).expect("failed to query Git revision");
  if let Some(git_rev) = git_rev {
    println!("cargo:rustc-env=VERSION={pkg_version} ({git_rev})");
  } else {
    println!("cargo:rustc-env=VERSION={pkg_version}");
  }
}
