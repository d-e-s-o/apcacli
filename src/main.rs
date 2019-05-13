// Copyright (C) 2019 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use apca::api::v1::account;
use apca::ApiInfo;
use apca::Client;
use apca::Error;

use futures::future::Future;
use futures::future::ok;

use structopt::StructOpt;

use tokio::runtime::current_thread::block_on_all;


/// A command line client for automated trading with Alpaca.
#[derive(Debug, StructOpt)]
enum Opts {
  /// Retrieve information about the Alpaca account.
  #[structopt(name = "account")]
  Account,
}


/// Format an account status.
fn format_account_status(status: account::Status) -> String {
  match status {
    account::Status::Onboarding => "onboarding",
    account::Status::SubmissionFailed => "submission failed",
    account::Status::Submitted => "submitted",
    account::Status::Updating => "updating",
    account::Status::ApprovalPending => "approval pending",
    account::Status::Active => "active",
    account::Status::Rejected => "rejected",
  }.to_string()
}


/// The handler for the 'account' command.
fn account(client: Client) -> Result<Box<dyn Future<Item = (), Error = Error>>, Error> {
  let fut = client
    .issue::<account::Get>(())?
    .map_err(Error::from)
    .and_then(|account| {
      println!(r#"account:
  id:                {id}
  status:            {status}
  buying power:      {buying_power} {currency}
  cash:              {cash} {currency}
  withdrawable cash: {withdrawable_cash} {currency}
  portfolio value:   {portfolio_value} {currency}
  day trader:        {day_trader}
  trading blocked:   {trading_blocked}
  transfers blocked: {transfers_blocked}
  account blocked:   {account_blocked}"#,
        id = account.id.to_hyphenated_ref(),
        status = format_account_status(account.status),
        currency = account.currency,
        buying_power = account.buying_power,
        cash = account.cash,
        withdrawable_cash = account.withdrawable_cash,
        portfolio_value = account.portfolio_value,
        day_trader = account.day_trader,
        trading_blocked = account.trading_blocked,
        transfers_blocked = account.transfers_blocked,
        account_blocked = account.account_blocked,
      );
      ok(())
    });

  Ok(Box::new(fut))
}


fn main() -> Result<(), Error> {
  let opts = Opts::from_args();
  let api_info = ApiInfo::from_env()?;
  let client = Client::new(api_info)?;

  let future = match opts {
    Opts::Account => account(client),
  }?;

  block_on_all(future)
}
