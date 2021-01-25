// Copyright (C) 2020 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Debug;
use std::str::FromStr;

use apca::api::v2::asset;
use apca::api::v2::order;

use num_decimal::Num;

use structopt::clap::ArgGroup;
use structopt::StructOpt;

use uuid::Error as UuidError;
use uuid::Uuid;


/// A command line client for automated trading with Alpaca.
#[derive(Debug, StructOpt)]
pub struct Args {
  #[structopt(subcommand)]
  pub command: Command,
  /// Increase verbosity (can be supplied multiple times).
  #[structopt(short = "v", long = "verbose", global = true, parse(from_occurrences))]
  pub verbosity: usize,
}

/// A command line client for automated trading with Alpaca.
#[derive(Debug, StructOpt)]
pub enum Command {
  /// Retrieve information about the Alpaca account.
  Account(Account),
  /// Retrieve information pertaining assets.
  Asset(Asset),
  /// Subscribe to some event stream.
  Events(Events),
  /// Retrieve status information about the market.
  Market,
  /// Perform various order related functions.
  Order(Order),
  /// Perform various position related functions.
  Position(Position),
}


/// An indication when/for how long an order is valid.
#[derive(Debug)]
pub enum TimeInForce {
  Today,
  Canceled,
  MarketOpen,
  MarketClose,
}

impl TimeInForce {
  pub fn to_time_in_force(&self) -> order::TimeInForce {
    match self {
      Self::Today => order::TimeInForce::Day,
      Self::Canceled => order::TimeInForce::UntilCanceled,
      Self::MarketOpen => order::TimeInForce::UntilMarketOpen,
      Self::MarketClose => order::TimeInForce::UntilMarketClose,
    }
  }
}

impl FromStr for TimeInForce {
  type Err = String;

  fn from_str(src: &str) -> Result<Self, Self::Err> {
    match src {
      "today" => Ok(Self::Today),
      "canceled" => Ok(Self::Canceled),
      "market-open" => Ok(Self::MarketOpen),
      "market-close" => Ok(Self::MarketClose),
      _ => Err(format!("invalid time-in-force specifier: {}", src)),
    }
  }
}


#[derive(Clone, Debug, PartialEq)]
pub struct Symbol(pub asset::Symbol);

impl FromStr for Symbol {
  type Err = String;

  fn from_str(sym: &str) -> Result<Self, Self::Err> {
    let sym =
      asset::Symbol::from_str(sym).map_err(|e| format!("failed to parse symbol {}: {}", sym, e))?;

    Ok(Symbol(sym))
  }
}


/// An enumeration representing the `account` command.
#[derive(Debug, StructOpt)]
pub enum Account {
  /// Query and print information about the account.
  Get,
  /// Retrieve account activity.
  Activity(Activity),
  /// Retrieve and modify the account configuration.
  Config(Config),
}

/// An enumeration representing the `account activity` sub command.
#[derive(Debug, StructOpt)]
pub enum Activity {
  /// Retrieve account activity.
  Get,
}

/// An enumeration representing the `account config` sub command.
#[derive(Debug, StructOpt)]
pub enum Config {
  /// Retrieve the account configuration.
  Get,
  /// Modify the account configuration.
  Set(ConfigSet),
}

#[derive(Debug, StructOpt)]
pub struct ConfigSet {
  /// Enable e-mail trade confirmations.
  #[structopt(short = "e", long)]
  pub confirm_email: bool,
  /// Disable e-mail trade confirmations.
  #[structopt(short = "E", long, conflicts_with("confirm-email"))]
  pub no_confirm_email: bool,
  /// Suspend trading.
  #[structopt(short = "t", long)]
  pub trading_suspended: bool,
  /// Resume trading.
  #[structopt(short = "T", long, conflicts_with("trading-suspended"))]
  pub no_trading_suspended: bool,
  /// Enable shorting.
  #[structopt(short = "s", long)]
  pub shorting: bool,
  /// Disable shorting.
  #[structopt(short = "S", long, conflicts_with("shorting"))]
  pub no_shorting: bool,
}


/// An enumeration representing the `asset` command.
#[derive(Debug, StructOpt)]
pub enum Asset {
  /// Query information about a specific asset.
  Get {
    /// The asset's symbol or ID.
    symbol: Symbol,
  },
  /// List all assets.
  List,
}


/// The type of event to stream.
#[derive(Debug, StructOpt)]
pub enum EventType {
  /// Subscribe to account events.
  Account,
  /// Subscribe to trade events.
  Trades,
}

/// A struct representing the `events` command.
#[derive(Debug, StructOpt)]
pub struct Events {
  #[structopt(flatten)]
  pub event: EventType,
  /// Print events in JSON format.
  #[structopt(short = "j", long)]
  pub json: bool,
}


/// An enumeration representing the `order` command.
#[derive(Debug, StructOpt)]
pub enum Order {
  /// Submit an order.
  #[structopt(group = ArgGroup::with_name("amount").required(true))]
  Submit(SubmitOrder),
  /// Change an order.
  #[structopt(group = ArgGroup::with_name("amount"))]
  Change(ChangeOrder),
  /// Cancel a single order (by id) or all open ones (via 'all').
  Cancel { cancel: CancelOrder },
  /// Retrieve information about a single order.
  Get {
    /// The ID of the order to retrieve information about.
    id: OrderId,
  },
  /// List orders.
  List {
    /// Show only closed orders instead of open ones.
    #[structopt(short = "c", long)]
    closed: bool,
  },
}


/// A type representing the options to submit an order.
#[derive(Debug, StructOpt)]
pub struct SubmitOrder {
  /// The side of the order.
  pub side: Side,
  /// The symbol of the asset involved in the order.
  pub symbol: String,
  /// The quantity to trade.
  #[structopt(long, group = "amount")]
  pub quantity: Option<u64>,
  /// The value to trade.
  #[structopt(long, group = "amount")]
  pub value: Option<Num>,
  /// Create a limit order (or stop limit order) with the given limit price.
  #[structopt(short = "l", long)]
  pub limit_price: Option<Num>,
  /// Create a stop order (or stop limit order) with the given stop price.
  #[structopt(short = "s", long)]
  pub stop_price: Option<Num>,
  /// Create a one-triggers-other order with the given take-profit price.
  #[structopt(long)]
  pub take_profit_price: Option<Num>,
  /// Create an order that is eligible to execute during
  /// pre-market/after hours. Note that only limit orders that are
  /// valid for the day are supported.
  #[structopt(long)]
  pub extended_hours: bool,
  /// When/for how long the order is valid ('today', 'canceled',
  /// 'market-open', or 'market-close').
  #[structopt(short = "t", long, default_value = "canceled")]
  pub time_in_force: TimeInForce,
}

/// A type representing the options to change an order.
#[derive(Debug, StructOpt)]
pub struct ChangeOrder {
  /// The ID of the order to change.
  pub id: OrderId,
  /// The quantity to trade.
  #[structopt(long, group = "amount")]
  pub quantity: Option<u64>,
  /// The value to trade.
  #[structopt(long, group = "amount")]
  pub value: Option<Num>,
  /// Create a limit order (or stop limit order) with the given limit price.
  #[structopt(short = "l", long)]
  pub limit_price: Option<Num>,
  /// Create a stop order (or stop limit order) with the given stop price.
  #[structopt(short = "s", long)]
  pub stop_price: Option<Num>,
  /// When/for how long the order is valid ('today', 'canceled',
  /// 'market-open', or 'market-close').
  #[structopt(short = "t", long)]
  pub time_in_force: Option<TimeInForce>,
}


/// An enumeration of the different options for order cancellation.
#[derive(Debug)]
pub enum CancelOrder {
  /// Cancel a single order as specified by an `OrderId`.
  ById(OrderId),
  /// Cancel all open orders.
  All,
}

impl FromStr for CancelOrder {
  type Err = UuidError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let cancel = match s {
      "all" => CancelOrder::All,
      s => CancelOrder::ById(OrderId::from_str(s)?),
    };
    Ok(cancel)
  }
}


#[derive(Debug, StructOpt)]
pub enum Side {
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
pub struct OrderId(pub order::Id);

impl FromStr for OrderId {
  type Err = UuidError;

  fn from_str(id: &str) -> Result<Self, Self::Err> {
    Ok(OrderId(order::Id(Uuid::parse_str(id)?)))
  }
}


#[derive(Debug, StructOpt)]
pub enum Position {
  /// Inquire information about the position holding a specific symbol.
  Get {
    /// The position's symbol.
    symbol: Symbol,
  },
  /// List all open positions.
  List,
  /// Liquidate a position for a certain asset.
  Close {
    /// The position's symbol.
    symbol: Symbol,
  },
}
