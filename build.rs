// Copyright (C) 2021 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::Command;

use anyhow::Context;
use anyhow::Result;

const GIT: &str = "git";


fn git_command(args: &[&str]) -> String {
  args.iter().fold(GIT.to_string() + " ", |mut cmd, arg| {
    cmd += &(" ".to_owned() + arg);
    cmd
  })
}


/// Run git with the provided arguments and read the output it emits.
fn git(args: &[&str]) -> Result<String> {
  let git = Command::new(GIT)
    .args(args)
    .output()
    .with_context(|| format!("failed to run `{}`", git_command(args)))?;

  let output = String::from_utf8(git.stdout).with_context(|| {
    format!(
      "failed to read `{}` output as UTF-8 string",
      git_command(args)
    )
  })?;

  Ok(output)
}


fn main() -> Result<()> {
  let revision = git(&["rev-parse", "--short", "HEAD"])?;
  println!(
    "cargo:rustc-env=VERSION={} ({})",
    env!("CARGO_PKG_VERSION"),
    revision.trim()
  );
  Ok(())
}
