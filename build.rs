// Copyright (C) 2021-2022 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::Command;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;

const GIT: &str = "git";


/// Format a git command with the given list of arguments as a string.
fn git_command(args: &[&str]) -> String {
  args.iter().fold(GIT.to_string(), |mut cmd, arg| {
    cmd += " ";
    cmd += arg;
    cmd
  })
}


/// Run git with the provided arguments and read the output it emits.
fn git_output(args: &[&str]) -> Result<String> {
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

/// Run git with the provided arguments and report the status of the
/// command.
fn git_run(args: &[&str]) -> Result<bool> {
  Command::new(GIT)
    .args(args)
    .status()
    .with_context(|| format!("failed to run `{}`", git_command(args)))
    .map(|status| status.success())
}


/// Create a suffix to add to the regular cargo reported program
/// version, which includes information about the git commit, if
/// available.
fn version_suffix() -> Result<String> {
  // As a first step we check whether we are in a git repository and
  // whether git is working to begin with. If not, we can't do much; yet
  // we still want to allow the build to continue, so we merely print a
  // warning and continue without a version suffix. But once these
  // checks are through, we treat subsequent failures as unexpected and
  // fatal.
  match git_run(&["rev-parse", "--git-dir"]) {
    Ok(true) => (),
    Ok(false) => {
      println!("cargo:warning=Not in a git repository; unable to embed git revision");
      return Ok(String::new())
    },
    Err(err) => {
      println!(
        "cargo:warning=Failed to invoke `git`; unable to embed git revision: {}",
        err
      );
      return Ok(String::new())
    },
  }

  let revision = git_output(&["rev-parse", "--short", "HEAD"])?;
  let suffix = format!(" ({})", revision.trim());
  Ok(suffix)
}

fn main() -> Result<()> {
  let version_suffix = version_suffix()?;
  println!(
    "cargo:rustc-env=VERSION={}{}",
    env!("CARGO_PKG_VERSION"),
    version_suffix
  );
  // Make sure to run this script again if any relevant version control
  // files changes (e.g., when creating a commit).
  println!("cargo:rerun-if-changed=.git/index");
  Ok(())
}
