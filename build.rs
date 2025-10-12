// Copyright (C) 2021-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::env;

use anyhow::Context as _;
use anyhow::Result;

use grev::git_revision_auto;


fn main() -> Result<()> {
  let manifest_dir =
    env::var_os("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR variable not set")?;
  let pkg_version = env::var("CARGO_PKG_VERSION").context("CARGO_PKG_VERSION variable not set")?;

  if let Some(git_rev) = git_revision_auto(manifest_dir)? {
    println!("cargo:rustc-env=VERSION={pkg_version} ({git_rev})");
  } else {
    println!("cargo:rustc-env=VERSION={pkg_version}");
  }
  Ok(())
}
