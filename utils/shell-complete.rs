// Copyright (C) 2020 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::stdout;

use structopt::clap::Shell;
use structopt::StructOpt;


#[allow(unused)]
mod apcacli {
  include!("../src/args.rs");
}


/// Generate a bash completion script for apcacli.
#[derive(Debug, StructOpt)]
pub struct Args {
  /// The command for which to generate the bash completion script.
  #[structopt(default_value = "apcacli")]
  pub command: String,
}


fn main() {
  let args = Args::from_args();
  let mut app = apcacli::Args::clap();
  app.gen_completions_to(&args.command, Shell::Bash, &mut stdout());
}
