// Copyright (C) 2021-2022 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::stdout;

use anyhow::Result;

use grev::get_revision as get_git_rev;


fn main() -> Result<()> {
  let dir = env!("CARGO_MANIFEST_DIR");
  if let Some(git_rev) = get_git_rev(dir, stdout().lock())? {
    println!(
      "cargo:rustc-env=VERSION={} ({})",
      env!("CARGO_PKG_VERSION"),
      git_rev
    );
  } else {
    println!("cargo:rustc-env=VERSION={}", env!("CARGO_PKG_VERSION"));
  }
  Ok(())
}
