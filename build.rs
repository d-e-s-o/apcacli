// Copyright (C) 2021-2022 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::Command;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;

const GIT: &str = "git";


/// Format a git command with the given list of arguments as a string.
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

  if !git.status.success() {
    let code = if let Some(code) = git.status.code() {
      format!(" ({})", code)
    } else {
      String::new()
    };

    bail!(
      "`{}` reported non-zero exit-status{}",
      git_command(args),
      code
    );
  }

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
