// Copyright (C) 2020-2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;
use std::fmt::Debug;
use std::str::FromStr;

use apca::api::v2::asset;
use apca::api::v2::order;
use apca::api::v2::watchlist;

use chrono::NaiveDate;
use chrono::NaiveDateTime;

use clap::ArgAction;
use clap::ArgGroup;
use clap::Args as ClapArgs;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;

use num_decimal::Num;

use uuid::Error as UuidError;
use uuid::Uuid;


/// A command line tool for trading stocks on Alpaca (alpaca.markets).
#[derive(Debug, Parser)]
#[clap(version = env!("VERSION"))]
pub struct Args {
  #[clap(subcommand)]
  pub command: Command,
  /// Increase verbosity (can be supplied multiple times).
  #[clap(short = 'v', long = "verbose", global = true, action = ArgAction::Count)]
  pub verbosity: u8,
}

/// A command line tool for trading stocks on Alpaca (alpaca.markets).
#[derive(Debug, Subcommand)]
pub enum Command {
  /// Retrieve information about the Alpaca account.
  #[clap(subcommand)]
  Account(Account),
  /// Retrieve information pertaining assets.
  #[clap(subcommand)]
  Asset(Asset),
  /// Retrieve historical aggregate bars for an asset.
  #[clap(subcommand)]
  Bars(Bars),
  /// Retrieve status information about the market.
  #[clap(value_enum)]
  Market,
  /// Perform various order related functions.
  #[clap(subcommand)]
  Order(Order),
  /// Perform various position related functions.
  #[clap(subcommand)]
  Position(Position),
  /// Subscribe to some update stream.
  #[clap(subcommand)]
  Updates(Updates),
  /// Manage asset/security watch lists.
  #[clap(subcommand)]
  Watchlist(Watchlist),
  /// An extension and its arguments.
  #[clap(external_subcommand)]
  Extension(Vec<OsString>),
}


/// An indication when/for how long an order is valid.
#[derive(Clone, Debug)]
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

    Ok(Self(sym))
  }
}


/// An enumeration representing the `account` command.
#[derive(Debug, Subcommand)]
pub enum Account {
  /// Query and print information about the account.
  #[clap(value_enum)]
  Get,
  /// Retrieve account activity.
  #[clap(subcommand)]
  Activity(Activity),
  /// Retrieve and modify the account configuration.
  #[clap(subcommand)]
  Config(Config),
}

/// An enumeration representing the `account activity` sub command.
#[derive(Debug, Subcommand)]
pub enum Activity {
  /// Retrieve account activity.
  #[clap(value_enum)]
  Get(ActivityGet),
}

#[derive(Debug, ClapArgs)]
pub struct ActivityGet {
  /// Only show activities dated at the given date or after (format:
  /// yyyy-mm-dd).
  #[clap(short, long)]
  pub begin: Option<NaiveDate>,
}

/// An enumeration representing the `account config` sub command.
#[derive(Debug, Subcommand)]
pub enum Config {
  /// Retrieve the account configuration.
  #[clap(value_enum)]
  Get,
  /// Modify the account configuration.
  Set(ConfigSet),
}

#[derive(Debug, ClapArgs)]
pub struct ConfigSet {
  /// Enable e-mail trade confirmations.
  #[clap(short = 'e', long)]
  pub confirm_email: bool,
  /// Disable e-mail trade confirmations.
  #[clap(short = 'E', long, conflicts_with("confirm-email"))]
  pub no_confirm_email: bool,
  /// Suspend trading.
  #[clap(short = 't', long)]
  pub trading_suspended: bool,
  /// Resume trading.
  #[clap(short = 'T', long, conflicts_with("trading-suspended"))]
  pub no_trading_suspended: bool,
  /// Enable shorting.
  #[clap(short = 's', long)]
  pub shorting: bool,
  /// Disable shorting.
  #[clap(short = 'S', long, conflicts_with("shorting"))]
  pub no_shorting: bool,
}


fn parse_asset_class(s: &str) -> Result<asset::Class, String> {
  let class =
    asset::Class::from_str(s).map_err(|()| format!("provided asset class '{s}' is invalid"))?;

  Ok(class)
}


/// An enumeration representing the `asset` command.
#[derive(Debug, Subcommand)]
pub enum Asset {
  /// Query information about a specific asset.
  Get {
    /// The asset's symbol or ID.
    symbol: Symbol,
  },
  /// List all assets.
  List {
    #[clap(
      short,
      long,
      default_value = asset::Class::UsEquity.as_ref(),
      value_parser = parse_asset_class,
    )]
    class: asset::Class,
  },
}


/// An indication when/for how long an order is valid.
#[derive(Clone, Debug)]
pub enum TimeFrame {
  /// Retrieve historical data aggregated per day.
  Day,
  /// Retrieve historical data aggregated per hour.
  Hour,
  /// Retrieve historical data aggregated per minute.
  Minute,
}

impl FromStr for TimeFrame {
  type Err = String;

  fn from_str(side: &str) -> Result<Self, Self::Err> {
    match side {
      "day" => Ok(TimeFrame::Day),
      "hour" => Ok(TimeFrame::Hour),
      "minute" => Ok(TimeFrame::Minute),
      s => Err(format!(
        "{} is not a valid time frame specification (use 'day', 'hour', or 'minute')",
        s
      )),
    }
  }
}


/// Parse a `DateTime` from a provided date or datetime string.
fn parse_date_time(s: &str) -> Result<NaiveDateTime, String> {
  match NaiveDateTime::from_str(s) {
    Ok(date_time) => Ok(date_time),
    Err(_) => NaiveDate::from_str(s)
      .map_err(|err| err.to_string())
      .and_then(|date| {
        date
          .and_hms_opt(0, 0, 0)
          .ok_or_else(|| format!("failed to parse date time string: '{s}'"))
      }),
  }
}


/// An enumeration representing the `bars` command.
#[derive(Debug, Subcommand)]
pub enum Bars {
  /// Retrieve historical aggregate bars for a symbol.
  Get {
    /// The asset for which to retrieve historical aggregate bars.
    symbol: String,
    /// The aggregation time frame.
    time_frame: TimeFrame,
    /// The start time for which to retrieve bars.
    #[clap(value_parser = parse_date_time)]
    start: NaiveDateTime,
    /// The end time for which to retrieve bars.
    #[clap(value_parser = parse_date_time)]
    end: NaiveDateTime,
  },
}


/// A enumeration of all supported realtime market data sources.
#[derive(Copy, Clone, Debug, Subcommand)]
pub enum DataSource {
  /// Use the Investors Exchange (IEX) as the data source.
  Iex,
  /// Use CTA (administered by NYSE) and UTP (administered by Nasdaq)
  /// SIPs as the data source.
  ///
  /// This source is only usable with the unlimited market data plan.
  Sip,
}

impl FromStr for DataSource {
  type Err = String;

  fn from_str(side: &str) -> Result<Self, Self::Err> {
    match side {
      "iex" => Ok(DataSource::Iex),
      "sip" => Ok(DataSource::Sip),
      s => Err(format!(
        "{} is not a valid data source (use 'iex' or 'sip')",
        s
      )),
    }
  }
}


/// A struct representing the `updates` command.
#[derive(Debug, Subcommand)]
pub enum Updates {
  /// Subscribe to trade events.
  Trades,
  /// Subscribe to realtime market data aggregates.
  Data {
    /// The symbols for which to receive aggregate data.
    symbols: Vec<String>,
    /// The data source to use.
    #[clap(long, default_value = "iex")]
    source: DataSource,
  },
}


/// An enumeration representing the `order` command.
#[derive(Debug, Subcommand)]
pub enum Order {
  /// Submit an order.
  #[clap(group = ArgGroup::new("amount").required(true))]
  Submit(SubmitOrder),
  /// Change an order.
  #[clap(group = ArgGroup::new("amount"))]
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
    #[clap(short = 'c', long)]
    closed: bool,
  },
}


/// A type representing the options to submit an order.
#[derive(Debug, ClapArgs)]
pub struct SubmitOrder {
  /// The side of the order.
  pub side: Side,
  /// The symbol of the asset involved in the order.
  pub symbol: String,
  /// The quantity to trade.
  #[clap(long, group = "amount")]
  pub quantity: Option<Num>,
  /// The value to trade.
  #[clap(long, group = "amount")]
  pub value: Option<Num>,
  /// Create a limit order (or stop limit order) with the given limit price.
  #[clap(short = 'l', long)]
  pub limit_price: Option<Num>,
  /// Create a stop order (or stop limit order) with the given stop price.
  #[clap(short = 's', long)]
  pub stop_price: Option<Num>,
  /// Create a one-triggers-other or bracket order with the given
  /// take-profit price.
  #[clap(long)]
  pub take_profit_price: Option<Num>,
  /// Create a one-triggers-other or bracket order with the given
  /// stop-price under the stop-loss advanced order leg.
  #[clap(long)]
  pub stop_loss_stop_price: Option<Num>,
  /// Create a one-triggers-other or bracket order with the given
  /// limit-price under the stop-loss advanced order leg. Note that this
  /// option can only be used in conjunction with stop-loss-stop-price.
  #[clap(long)]
  pub stop_loss_limit_price: Option<Num>,
  /// Create an order that is eligible to execute during
  /// pre-market/after hours. Note that only limit orders that are
  /// valid for the day are supported.
  #[clap(long)]
  pub extended_hours: bool,
  /// When/for how long the order is valid ('today', 'canceled',
  /// 'market-open', or 'market-close').
  #[clap(short = 't', long, default_value = "canceled")]
  pub time_in_force: TimeInForce,
}

/// A type representing the options to change an order.
#[derive(Debug, ClapArgs)]
pub struct ChangeOrder {
  /// The ID of the order to change.
  pub id: OrderId,
  /// The quantity to trade.
  #[clap(long, group = "amount")]
  pub quantity: Option<Num>,
  /// The value to trade.
  #[clap(long, group = "amount")]
  pub value: Option<Num>,
  /// Create a limit order (or stop limit order) with the given limit price.
  #[clap(short = 'l', long)]
  pub limit_price: Option<Num>,
  /// Create a stop order (or stop limit order) with the given stop price.
  #[clap(short = 's', long)]
  pub stop_price: Option<Num>,
  /// When/for how long the order is valid ('today', 'canceled',
  /// 'market-open', or 'market-close').
  #[clap(short = 't', long)]
  pub time_in_force: Option<TimeInForce>,
}


/// An enumeration of the different options for order cancellation.
#[derive(Clone, Debug)]
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


#[derive(Clone, Debug, ValueEnum)]
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


#[derive(Clone, Debug)]
pub struct OrderId(pub order::Id);

impl FromStr for OrderId {
  type Err = UuidError;

  fn from_str(id: &str) -> Result<Self, Self::Err> {
    Ok(OrderId(order::Id(Uuid::parse_str(id)?)))
  }
}


#[derive(Debug, Subcommand)]
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


/// Parse a comma-separated list of symbols.
fn parse_symbol_list(s: &str) -> Result<Vec<String>, String> {
  let symbols = s
    .split(',')
    .map(str::trim)
    .map(str::to_string)
    .collect::<Vec<_>>();
  Ok(symbols)
}


#[derive(Clone, Debug)]
pub struct WatchlistId(pub watchlist::Id);

impl FromStr for WatchlistId {
  type Err = UuidError;

  fn from_str(id: &str) -> Result<Self, Self::Err> {
    Ok(WatchlistId(watchlist::Id(Uuid::parse_str(id)?)))
  }
}


/// A type representing the options to create a watch list.
#[derive(Debug, ClapArgs)]
pub struct CreateWatchlist {
  /// The name of the watch list.
  pub name: String,
  /// The list of symbols to put into the newly create watch list.
  pub symbols: Vec<String>,
}


/// A type representing the options to update a watch list.
#[derive(Debug, ClapArgs)]
pub struct UpdateWatchlist {
  /// The watch list's ID.
  pub id: WatchlistId,
  /// The new name of the watch list.
  #[clap(short, long)]
  pub name: Option<String>,
  /// Add the given list of symbols to the watch list.
  #[clap(short, long, value_parser = parse_symbol_list)]
  pub add: Vec<Vec<String>>,
  /// Remove the given list of symbols from the watch list.
  #[clap(short, long, value_parser = parse_symbol_list)]
  pub remove: Vec<Vec<String>>,
}


#[derive(Debug, Subcommand)]
pub enum Watchlist {
  /// Create a new watch list.
  Create(CreateWatchlist),
  /// List all watch lists.
  List,
  /// Inquire information about a watch list.
  Get {
    /// The watch list's ID.
    id: WatchlistId,
  },
  /// Update an existing watch list.
  Update(UpdateWatchlist),
  /// Delete a watch list.
  Delete {
    /// The watch list's ID.
    id: WatchlistId,
  },
}
