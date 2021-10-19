// Copyright (C) 2020-2021 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::large_enum_variant, clippy::let_and_return)]

use std::io::stdout;

use structopt::clap::Shell;
use structopt::StructOpt;


#[allow(unused)]
mod apcacli {
  include!("../src/args.rs");
}


/// Generate a shell completion script for apcacli.
#[derive(Debug, StructOpt)]
struct Args {
  /// The shell for which to generate a completion script for.
  #[structopt(possible_values = &Shell::variants())]
  shell: Shell,
  /// The command for which to generate the shell completion script.
  #[structopt(default_value = "apcacli")]
  command: String,
}


fn main() {
  let args = Args::from_args();
  let mut app = apcacli::Args::clap();
  app.gen_completions_to(&args.command, args.shell, &mut stdout());
}
