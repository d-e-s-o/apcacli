// Copyright (C) 2020-2021 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::large_enum_variant, clippy::let_and_return)]

use std::io::stdout;

use clap::IntoApp;
use clap::Parser;

use clap_complete::generate;
use clap_complete::Shell;


#[allow(unused)]
mod apcacli {
  include!("../src/args.rs");
}


/// Generate a shell completion script for apcacli.
#[derive(Debug, Parser)]
struct Args {
  /// The shell for which to generate a completion script for.
  #[clap(arg_enum)]
  shell: Shell,
  /// The command for which to generate the shell completion script.
  #[clap(default_value = "apcacli")]
  command: String,
}


fn main() {
  let args = Args::parse();
  let mut app = apcacli::Args::command();
  generate(
    args.shell,
    &mut app,
    &args.command,
    &mut stdout()
  );
}
