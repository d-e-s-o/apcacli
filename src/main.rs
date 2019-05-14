// Copyright (C) 2019 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::str::FromStr;

use apca::api::v1::account;
use apca::api::v1::asset;
use apca::api::v1::order;
use apca::ApiInfo;
use apca::Client;
use apca::Error;

use futures::future::Future;
use futures::future::ok;

use num_decimal::Num;

use simplelog::Config;
use simplelog::LevelFilter;
use simplelog::SimpleLogger;

use structopt::StructOpt;

use tokio::runtime::current_thread::block_on_all;

use uuid::parser::ParseError;
use uuid::Uuid;


/// A command line client for automated trading with Alpaca.
#[derive(Debug, StructOpt)]
struct Opts {
  #[structopt(subcommand)]
  command: Command,
  /// Increase verbosity (can be supplied multiple times).
  #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
  verbosity: usize,
}

/// A command line client for automated trading with Alpaca.
#[derive(Debug, StructOpt)]
enum Command {
  /// Retrieve information about the Alpaca account.
  #[structopt(name = "account")]
  Account,
  /// Perform various order related functions.
  #[structopt(name = "order")]
  Order(Order),
}


#[derive(Debug, StructOpt)]
enum Order {
  /// Submit an order.
  #[structopt(name = "submit")]
  Submit {
    /// The side of the order.
    side: Side,
    /// The symbol of the asset involved in the order.
    symbol: String,
    /// The quantity to trade.
    quantity: u64,
    /// Create a limit order (or stop limit order) with the given limit price.
    #[structopt(short = "l", long = "limit")]
    limit_price: Option<Num>,
    /// Create a stop order (or stop limit order) with the given stop price.
    #[structopt(short = "s", long = "stop")]
    stop_price: Option<Num>,
    /// Create an order that is only valid for today.
    #[structopt(long = "today")]
    today: bool,
  },
  /// Cancel an order.
  #[structopt(name = "cancel")]
  Cancel { id: OrderId },
}


#[derive(Debug, StructOpt)]
enum Side {
  /// Buy an asset.
  Buy,
  /// Sell an asset.
  Sell,
}

impl FromStr for Side {
  type Err = String;

  fn from_str(side: &str) -> Result<Self, Self::Err> {
    match side {
      "buy" => Ok(Side::Buy),
      "sell" => Ok(Side::Sell),
      s => Err(format!(
        "{} is not a valid side specification (use 'buy' or 'sell')",
        s
      )),
    }
  }
}


#[derive(Debug)]
struct OrderId(order::Id);

impl FromStr for OrderId {
  type Err = ParseError;

  fn from_str(id: &str) -> Result<Self, Self::Err> {
    Ok(OrderId(order::Id(Uuid::parse_str(id)?)))
  }
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


fn order(client: Client, order: Order) -> Result<Box<dyn Future<Item = (), Error = Error>>, Error> {
  match order {
    Order::Submit {
      side,
      symbol,
      quantity,
      limit_price,
      stop_price,
      today,
    } => {
      let side = match side {
        Side::Buy => order::Side::Buy,
        Side::Sell => order::Side::Sell,
      };

      let type_ = match (limit_price.is_some(), stop_price.is_some()) {
        (true, true) => order::Type::StopLimit,
        (true, false) => order::Type::Limit,
        (false, true) => order::Type::Stop,
        (false, false) => order::Type::Market,
      };

      let time_in_force = if today {
        order::TimeInForce::Day
      } else {
        order::TimeInForce::UntilCanceled
      };

      let request = order::OrderReq {
        // TODO: We should probably support other forms of specifying
        //       the symbol.
        symbol: asset::Symbol::Sym(symbol),
        quantity,
        side,
        type_,
        time_in_force,
        limit_price,
        stop_price,
      };

      let fut = client
        .issue::<order::Post>(request)?
        .map_err(Error::from)
        .and_then(|order| {
          println!("{}", order.id.to_hyphenated_ref());
          ok(())
        });

      Ok(Box::new(fut))
    },
    Order::Cancel { id } => {
      let fut = client.issue::<order::Delete>(id.0)?.map_err(Error::from);
      Ok(Box::new(fut))
    },
  }
}


fn main() -> Result<(), Error> {
  let opts = Opts::from_args();
  let level = match opts.verbosity {
    0 => LevelFilter::Warn,
    1 => LevelFilter::Info,
    2 => LevelFilter::Debug,
    _ => LevelFilter::Trace,
  };

  let _ = SimpleLogger::init(level, Config::default());
  let api_info = ApiInfo::from_env()?;
  let client = Client::new(api_info)?;

  let future = match opts.command {
    Command::Account => account(client),
    Command::Order(order) => self::order(client, order),
  }?;

  block_on_all(future)
}
