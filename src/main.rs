// Copyright (C) 2019-2020 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

#![type_length_limit = "2097152"]

use std::borrow::Cow;
use std::cmp::max;
use std::convert::TryInto;
use std::fmt::Debug;
use std::io::stdout;
use std::io::Write;
use std::process::exit;
use std::str::FromStr;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use apca::api::v2::account;
use apca::api::v2::asset;
use apca::api::v2::assets;
use apca::api::v2::clock;
use apca::api::v2::events;
use apca::api::v2::order;
use apca::api::v2::orders;
use apca::api::v2::position;
use apca::api::v2::positions;
use apca::ApiInfo;
use apca::Client;

use anyhow::Context;
use anyhow::Error;

use chrono::offset::Local;
use chrono::offset::TimeZone;

use futures::future::ready;
use futures::future::TryFutureExt;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;

use num_decimal::Num;

use structopt::StructOpt;

use tokio::runtime::Runtime;

use tracing::subscriber::set_global_default as set_global_subscriber;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::FmtSubscriber;

use uuid::Error as UuidError;
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
  /// Retrieve information pertaining assets.
  #[structopt(name = "asset")]
  Asset(Asset),
  /// Subscribe to some event stream.
  Events(Events),
  /// Retrieve status information about the market.
  Market,
  /// Perform various order related functions.
  #[structopt(name = "order")]
  Order(Order),
  /// Perform various position related functions.
  #[structopt(name = "position")]
  Position(Position),
}


/// An indication until when an order is valid.
#[derive(Debug)]
enum GoodUntil {
  Today,
  Canceled,
}

impl GoodUntil {
  pub fn to_time_in_force(&self) -> order::TimeInForce {
    match self {
      Self::Today => order::TimeInForce::Day,
      Self::Canceled => order::TimeInForce::UntilCanceled,
    }
  }
}

impl FromStr for GoodUntil {
  type Err = String;

  fn from_str(src: &str) -> Result<Self, Self::Err> {
    return match src {
      "today" => Ok(Self::Today),
      "canceled" => Ok(Self::Canceled),
      _ => Err(format!("invalid good-until specifier: {}", src)),
    }
  }
}


#[derive(Clone, Debug, PartialEq)]
struct Symbol(asset::Symbol);

impl FromStr for Symbol {
  type Err = String;

  fn from_str(sym: &str) -> Result<Self, Self::Err> {
    let sym =
      asset::Symbol::from_str(sym).map_err(|e| format!("failed to parse symbol {}: {}", sym, e))?;

    Ok(Symbol(sym))
  }
}


/// An enumeration representing the `asset` command.
#[derive(Debug, StructOpt)]
enum Asset {
  /// Query information about a specific asset.
  #[structopt(name = "get")]
  Get {
    /// The asset's symbol or ID.
    symbol: Symbol,
  },
  /// List all assets.
  #[structopt(name = "list")]
  List,
}


/// An enumeration representing the `events` command.
#[derive(Debug, StructOpt)]
enum Events {
  /// Subscribe to account events.
  #[structopt(name = "account")]
  Account,
  /// Subscribe to trade events.
  #[structopt(name = "trades")]
  Trades,
}


/// An enumeration representing the `order` command.
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
    /// Create an order that is eligible to execute during
    /// pre-market/after hours. Note that only limit orders that are
    /// valid for the day are supported.
    #[structopt(long = "extended-hours")]
    extended_hours: bool,
    /// How long the order will remain valid ('today' or 'canceled').
    #[structopt(short = "u", long = "good-until")]
    good_until: Option<GoodUntil>,
  },
  /// Cancel a single order (by id) or all open ones (via 'all').
  #[structopt(name = "cancel")]
  Cancel { cancel: CancelOrder },
  /// List orders.
  #[structopt(name = "list")]
  List,
}


/// An enumeration of the different options for order cancellation.
#[derive(Debug)]
enum CancelOrder {
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
  type Err = UuidError;

  fn from_str(id: &str) -> Result<Self, Self::Err> {
    Ok(OrderId(order::Id(Uuid::parse_str(id)?)))
  }
}


#[derive(Debug, StructOpt)]
enum Position {
  /// Inquire information about the position holding a specific symbol.
  #[structopt(name = "get")]
  Get {
    /// The position's symbol.
    symbol: Symbol,
  },
  /// List all open positions.
  #[structopt(name = "list")]
  List,
  /// Liquidate a position for a certain asset.
  #[structopt(name = "close")]
  Close {
    /// The position's symbol.
    symbol: Symbol,
  },
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
async fn account(client: Client) -> Result<(), Error> {
  let account = client
    .issue::<account::Get>(())
    .await
    .with_context(|| "failed to retrieve account information")?;

  println!(r#"account:
  id:                 {id}
  status:             {status}
  buying power:       {buying_power} {currency}
  cash:               {cash} {currency}
  long value:         {value_long} {currency}
  short value:        {value_short} {currency}
  equity:             {equity} {currency}
  last equity:        {last_equity} {currency}
  margin multiplier:  {multiplier}
  initial margin:     {initial_margin} {currency}
  maintenance margin: {maintenance_margin} {currency}
  day trade count:    {day_trade_count}
  day trader:         {day_trader}
  shorting enabled:   {shorting_enabled}
  trading suspended:  {trading_suspended}
  trading blocked:    {trading_blocked}
  transfers blocked:  {transfers_blocked}
  account blocked:    {account_blocked}"#,
    id = account.id.to_hyphenated_ref(),
    status = format_account_status(account.status),
    currency = account.currency,
    buying_power = account.buying_power,
    cash = account.cash,
    value_long = account.market_value_long,
    value_short = account.market_value_short,
    equity = account.equity,
    last_equity = account.last_equity,
    multiplier = account.multiplier,
    initial_margin = account.initial_margin,
    maintenance_margin = account.maintenance_margin,
    day_trade_count = account.daytrade_count,
    day_trader = account.day_trader,
    shorting_enabled = account.shorting_enabled,
    trading_suspended = account.trading_suspended,
    trading_blocked = account.trading_blocked,
    transfers_blocked = account.transfers_blocked,
    account_blocked = account.account_blocked,
  );
  Ok(())
}


/// The handler for the 'asset' command.
async fn asset(client: Client, asset: Asset) -> Result<(), Error> {
  match asset {
    Asset::Get { symbol } => asset_get(client, symbol).await,
    Asset::List => asset_list(client).await,
  }
}

/// Print information about the asset with the given symbol.
async fn asset_get(client: Client, symbol: Symbol) -> Result<(), Error> {
  let asset = client
    .issue::<asset::Get>(symbol.0.clone())
    .await
    .with_context(|| format!("failed to retrieve asset information for {}", symbol.0))?;

  println!(r#"{sym}:
  id:              {id}
  asset class:     {cls}
  exchange:        {exchg}
  status:          {status}
  tradable:        {tradable}
  marginable:      {marginable}
  shortable:       {shortable}
  easy-to-borrow:  {easy_to_borrow}"#,
    sym = asset.symbol,
    id = asset.id.to_hyphenated_ref(),
    cls = asset.class.as_ref(),
    exchg = asset.exchange.as_ref(),
    status = asset.status.as_ref(),
    tradable = asset.tradable,
    marginable = asset.marginable,
    shortable = asset.shortable,
    easy_to_borrow = asset.easy_to_borrow,
  );
  Ok(())
}

/// Print all tradable assets.
async fn asset_list(client: Client) -> Result<(), Error> {
  let request = assets::AssetsReq {
    status: asset::Status::Active,
    class: asset::Class::UsEquity,
  };
  let mut assets = client
    .issue::<assets::Get>(request)
    .await
    .with_context(|| "failed to retrieve asset list")?;

  assets.sort_by(|x, y| x.symbol.cmp(&y.symbol));

  let sym_max = max_width(&assets, |a| a.symbol.len());

  for asset in assets.into_iter().filter(|asset| asset.tradable) {
    println!(
      "{sym:<sym_width$} {id}",
      sym = asset.symbol,
      sym_width = sym_max,
      id = asset.id.to_hyphenated_ref(),
    );
  }
  Ok(())
}

/// Format a system time as per RFC 2822.
fn format_time(time: &SystemTime) -> Cow<'static, str> {
  match time.duration_since(UNIX_EPOCH) {
    Ok(duration) => {
      let secs = duration.as_secs().try_into().unwrap();
      let nanos = duration.subsec_nanos();
      Local.timestamp(secs, nanos).to_rfc2822().into()
    },
    Err(..) => "N/A".into(),
  }
}


async fn stream_account_updates(client: Client) -> Result<(), Error> {
  client
    .subscribe::<events::AccountUpdates>()
    .await
    .with_context(|| "failed to subscribe to account updates")?
    .try_for_each(|result| {
      async {
        let update = result.unwrap();
        println!(r#"account update:
  status:        {status}
  created at:    {created}
  updated at:    {updated}
  deleted at:    {deleted}
  cash:          {cash} {currency}
  withdrawable:  {withdrawable} {currency}
"#,
          status = update.status,
          currency = update.currency,
          created = update
            .created_at
            .as_ref()
            .map(format_time)
            .unwrap_or_else(|| "N/A".into()),
          updated = update
            .updated_at
            .as_ref()
            .map(format_time)
            .unwrap_or_else(|| "N/A".into()),
          deleted = update
            .deleted_at
            .as_ref()
            .map(format_time)
            .unwrap_or_else(|| "N/A".into()),
          cash = update.cash,
          withdrawable = update.withdrawable_cash,
        );
        Ok(())
      }
    })
    .await?;

  Ok(())
}


fn format_trade_status(status: events::TradeStatus) -> &'static str {
  match status {
    events::TradeStatus::New => "new",
    events::TradeStatus::PartialFill => "partially filled",
    events::TradeStatus::Filled => "filled",
    events::TradeStatus::DoneForDay => "done for day",
    events::TradeStatus::Canceled => "canceled",
    events::TradeStatus::Expired => "expired",
    events::TradeStatus::PendingCancel => "pending cancel",
    events::TradeStatus::Stopped => "stopped",
    events::TradeStatus::Rejected => "rejected",
    events::TradeStatus::Suspended => "suspended",
    events::TradeStatus::PendingNew => "pending new",
    events::TradeStatus::Calculated => "calculated",
  }
}

fn format_order_status(status: order::Status) -> &'static str {
  match status {
    order::Status::New => "new",
    order::Status::PartiallyFilled => "partially filled",
    order::Status::Filled => "filled",
    order::Status::DoneForDay => "done for day",
    order::Status::Canceled => "canceled",
    order::Status::Expired => "expired",
    order::Status::Accepted => "accepted",
    order::Status::PendingNew => "pending new",
    order::Status::AcceptedForBidding => "accepted for bidding",
    order::Status::PendingCancel => "pending cancel",
    order::Status::Stopped => "stopped",
    order::Status::Rejected => "rejected",
    order::Status::Suspended => "suspended",
    order::Status::Calculated => "calculated",
  }
}

fn format_order_type(type_: order::Type) -> &'static str {
  match type_ {
    order::Type::Market => "market",
    order::Type::Limit => "limit",
    order::Type::Stop => "stop",
    order::Type::StopLimit => "stop-limit",
  }
}

fn format_order_side(side: order::Side) -> &'static str {
  match side {
    order::Side::Buy => "buy",
    order::Side::Sell => "sell",
  }
}

async fn stream_trade_updates(client: Client) -> Result<(), Error> {
  client
    .subscribe::<events::TradeUpdates>()
    .await
    .with_context(|| "failed to subscribe to trade updates")?
    .try_for_each(|result| {
      async {
        let update = result.unwrap();
        println!(r#"{symbol} {status}:
  order id:      {id}
  status:        {order_status}
  type:          {type_}
  side:          {side}
  quantity:      {quantity}
  filled:        {filled}
"#,
          symbol = update.order.symbol,
          status = format_trade_status(update.event),
          id = update.order.id.to_hyphenated_ref(),
          order_status = format_order_status(update.order.status),
          type_ = format_order_type(update.order.type_),
          side = format_order_side(update.order.side),
          quantity = update.order.quantity.round(),
          filled = update.order.filled_quantity.round(),
        );
        Ok(())
      }
    })
    .await?;

  Ok(())
}

async fn events(client: Client, events: Events) -> Result<(), Error> {
  match events {
    Events::Account => stream_account_updates(client).await,
    Events::Trades => stream_trade_updates(client).await,
  }
}

/// Print the current market status.
async fn market(client: Client) -> Result<(), Error> {
  let clock = client
    .issue::<clock::Get>(())
    .await
    .with_context(|| "failed to retrieve market clock")?;

  println!(r#"market:
  open:         {open}
  current time: {current}
  next open:    {next_open}
  next close:   {next_close}"#,
    open = clock.open,
    current = format_time(&clock.current),
    next_open = format_time(&clock.next_open),
    next_close = format_time(&clock.next_close),
  );
  Ok(())
}


/// The handler for the 'order' command.
async fn order(client: Client, order: Order) -> Result<(), Error> {
  match order {
    Order::Submit {
      side,
      symbol,
      quantity,
      limit_price,
      stop_price,
      extended_hours,
      good_until,
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

      let time_in_force = good_until.unwrap_or(GoodUntil::Canceled).to_time_in_force();

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
        extended_hours,
      };

      let order = client
        .issue::<order::Post>(request)
        .await
        .with_context(|| "failed to submit order")?;

      println!("{}", order.id.to_hyphenated_ref());
      Ok(())
    },
    Order::Cancel { cancel } => order_cancel(client, cancel).await,
    Order::List => order_list(client).await,
  }
}

/// Cancel an open order.
async fn order_cancel(client: Client, cancel: CancelOrder) -> Result<(), Error> {
  match cancel {
    CancelOrder::ById(id) => client
      .issue::<order::Delete>(id.0)
      .await
      .with_context(|| "failed to cancel order"),
    CancelOrder::All => {
      let request = orders::OrdersReq { limit: 500 };
      let orders = client
        .issue::<orders::Get>(request)
        .await
        .with_context(|| "failed to list orders")?;

      orders
        .into_iter()
        .map(|order| {
          client.issue::<order::Delete>(order.id).map_err(move |e| {
            let id = order.id.to_hyphenated_ref();
            Error::new(e).context(format!("failed to cancel order {}", id))
          })
        })
        .collect::<FuturesUnordered<_>>()
        .fold(Ok(()), |acc, res| ready(acc.and(res)))
        .await
    },
  }
}


/// Determine the maximum width of values produced by applying a
/// function on each element of a slice.
fn max_width<T, F>(slice: &[T], f: F) -> usize
where
  F: Fn(&T) -> usize,
{
  slice.iter().fold(0, |m, i| max(m, f(&i)))
}


/// Format a quantity.
fn format_quantity(quantity: &Num) -> String {
  format!("{:.0}", quantity)
}


/// List all currently open orders.
async fn order_list(client: Client) -> Result<(), Error> {
  let account = client
    .issue::<account::Get>(())
    .await
    .with_context(|| "failed to retrieve account information")?;

  let request = orders::OrdersReq { limit: 500 };
  let mut orders = client
    .issue::<orders::Get>(request)
    .await
    .with_context(|| "failed to list orders")?;

  orders.sort_by(|a, b| a.symbol.cmp(&b.symbol));

  let currency = account.currency;
  let qty_max = max_width(&orders, |p| format_quantity(&p.quantity).len());
  let sym_max = max_width(&orders, |p| p.symbol.len());

  for order in orders {
    let price = match (order.limit_price, order.stop_price) {
      (Some(limit), Some(stop)) => {
        debug_assert!(order.type_ == order::Type::StopLimit, "{:?}", order.type_);
        format!("stop @ {} {}, limit @ {} {}", stop, currency, limit, currency)
      },
      (Some(limit), None) => {
        debug_assert!(order.type_ == order::Type::Limit, "{:?}", order.type_);
        format!("limit @ {} {}", limit, currency)
      },
      (None, Some(stop)) => {
        debug_assert!(order.type_ == order::Type::Stop, "{:?}", order.type_);
        format!("stop @ {} {}", stop, currency)
      },
      (None, None) => {
        debug_assert!(order.type_ == order::Type::Market, "{:?}", order.type_);
        "".to_string()
      },
    };

    println!(
      "{id} {side:>4} {qty:>qty_width$} {sym:<sym_width$} {price}",
      id = order.id.to_hyphenated_ref(),
      side = format_order_side(order.side),
      qty_width = qty_max,
      qty = format!("{:.0}", order.quantity),
      sym_width = sym_max,
      sym = order.symbol,
      price = price,
    )
  }
  Ok(())
}


/// The handler for the 'position' command.
async fn position(client: Client, position: Position) -> Result<(), Error> {
  match position {
    Position::Close { symbol } => position_close(client, symbol).await,
    Position::Get { symbol } => position_get(client, symbol).await,
    Position::List => position_list(client).await,
  }
}

fn format_position_side(side: position::Side) -> &'static str {
  match side {
    position::Side::Long => "long",
    position::Side::Short => "short",
  }
}

/// Retrieve and print a position for a given symbol.
async fn position_get(client: Client, symbol: Symbol) -> Result<(), Error> {
  let currency = client
    .issue::<account::Get>(())
    .await
    .with_context(|| "failed to retrieve account information")?
    .currency;

  let position = client
    .issue::<position::Get>(symbol.0.clone())
    .await
    .with_context(|| format!("failed to retrieve position for {}", symbol.0))?;

  println!(r#"{sym}:
  asset id:               {id}
  exchange:               {exchg}
  avg entry:              {entry} {currency}
  quantity:               {qty}
  side:                   {side}
  market value:           {value} {currency}
  cost basis:             {cost_basis} {currency}
  unrealized gain:        {unrealized_gain} {currency} ({unrealized_gain_pct}%)
  unrealized gain today:  {unrealized_gain_today} {currency} ({unrealized_gain_today_pct}%)
  current price:          {current_price} {currency}
  last price:             {last_price} {currency}"#,
    sym = position.symbol,
    id = position.asset_id.to_hyphenated_ref(),
    exchg = position.exchange.as_ref(),
    entry = format_price(&position.average_entry_price),
    currency = currency,
    qty = position.quantity,
    side = format_position_side(position.side),
    value = format_price(&position.market_value),
    cost_basis = format_price(&position.cost_basis),
    unrealized_gain = format_price(&position.unrealized_gain_total),
    unrealized_gain_pct = format_percent(&position.unrealized_gain_total_percent),
    unrealized_gain_today = format_price(&position.unrealized_gain_today),
    unrealized_gain_today_pct = format_percent(&position.unrealized_gain_today_percent),
    current_price = format_price(&position.current_price),
    last_price = format_price(&position.last_day_price),
  );

  Ok(())
}


/// Liquidate a position for a certain asset.
async fn position_close(client: Client, symbol: Symbol) -> Result<(), Error> {
  let currency = client
    .issue::<account::Get>(())
    .await
    .with_context(|| "failed to retrieve account information")?
    .currency;

  let order = client
    .issue::<position::Delete>(symbol.0.clone())
    .await
    .with_context(|| format!("failed to liquidate position for {}", symbol.0))?;

  println!(r#"{sym}:
  order id:         {id}
  status:           {status}
  quantity:         {quantity}
  filled quantity:  {filled}
  type:             {type_}
  side:             {side}
  limit:            {limit} {currency}
  stop:             {stop} {currency}"#,
    sym = order.symbol,
    id = order.id.to_hyphenated_ref(),
    status = format_order_status(order.status),
    quantity = order.quantity,
    filled = order.filled_quantity,
    type_ = format_order_type(order.type_),
    side = format_order_side(order.side),
    limit = order
      .limit_price
      .as_ref()
      .map(format_price)
      .unwrap_or_else(|| "N/A".into()),
    stop = order
      .stop_price
      .as_ref()
      .map(format_price)
      .unwrap_or_else(|| "N/A".into()),
    currency = currency,
  );
  Ok(())
}


/// Format a price value.
///
/// Note that this is really only the actual value without any currency.
fn format_price(price: &Num) -> String {
  format!("{:.2}", price)
}


/// Format a percentage value.
///
/// Note that this is really only the actual value, the percent sign is
/// omitted because clients may need to know size of the actual value
/// only.
fn format_percent(percent: &Num) -> String {
  format!("{:.2}", percent * 100)
}

/// Print a table with the given positions.
fn position_print(positions: &[position::Position], currency: &str) {
  let qty_max = max_width(&positions, |p| format_quantity(&p.quantity).len());
  let sym_max = max_width(&positions, |p| p.symbol.len());
  let price_max = max_width(&positions, |p| format_price(&p.current_price).len());
  let entry_max = max_width(&positions, |p| format_price(&p.average_entry_price).len());
  let today_max = max_width(&positions, |p| format_price(&p.unrealized_gain_today).len());
  let today_pct_max = max_width(&positions, |p| {
    format_percent(&p.unrealized_gain_today_percent).len()
  });
  let total_max = max_width(&positions, |p| format_price(&p.unrealized_gain_total).len());
  let total_pct_max = max_width(&positions, |p| {
    format_percent(&p.unrealized_gain_total_percent).len()
  });

  // We also need to take the total values into consideration for the
  // maximum width calculation.
  let today_gain = positions
    .iter()
    .fold(Num::default(), |acc, p| acc + &p.unrealized_gain_today);
  let base_value = positions
    .iter()
    .fold(Num::default(), |acc, p| acc + &p.cost_basis);
  let total_value = positions
    .iter()
    .fold(Num::default(), |acc, p| acc + &p.market_value);
  let total_gain = &total_value - &base_value;
  let last_value = &total_value - &today_gain;
  let (last_pct, total_gain_pct) = if base_value.is_zero() {
    (base_value.clone(), base_value.clone())
  } else {
    (
      &last_value / &base_value - 1,
      &total_value / &base_value - 1,
    )
  };
  let today_gain_pct = &total_gain_pct - &last_pct;

  let entry_max = max(entry_max, format_price(&base_value).len());
  let today_max = max(today_max, format_price(&today_gain).len());
  let today_pct_max = max(today_pct_max, format_percent(&today_gain_pct).len());
  let total_max = max(total_max, format_price(&total_gain).len());
  let total_pct_max = max(total_pct_max, format_percent(&total_gain_pct).len());

  // TODO: Strictly speaking we should also take into account the
  //       length of the formatted current value.
  let position_col = qty_max + 1 + sym_max + 3 + price_max + 1 + currency.len();
  let entry = "Avg Entry";
  let entry_col = max(entry_max + 1 + currency.len(), entry.len());
  let today = "Today P/L";
  let today_col = max(
    today_max + 1 + currency.len() + 2 + today_pct_max + 2,
    today.len(),
  );
  let total = "Total P/L";
  let total_col = max(
    total_max + 1 + currency.len() + 2 + total_pct_max + 2,
    total.len(),
  );

  println!(
    "{empty:^pos_width$} | {entry:^entry_width$} | {today:^today_width$} | {total:^total_width$}",
    empty = "",
    pos_width = position_col,
    entry_width = entry_col,
    entry = entry,
    today_width = today_col,
    today = today,
    total_width = total_col,
    total = total,
  );

  for position in positions {
    println!(
      "{qty:>qty_width$} {sym:<sym_width$} @ {price:>price_width$.2} {currency} | \
       {entry:>entry_width$} {currency} | \
       {today:>today_width$} {currency} ({today_pct:>today_pct_width$}%) | \
       {total:>total_width$} {currency} ({total_pct:>total_pct_width$}%)",
      qty_width = qty_max,
      qty = position.quantity,
      sym_width = sym_max,
      sym = position.symbol,
      price_width = price_max,
      price = position.current_price,
      currency = currency,
      entry_width = entry_max,
      entry = format_price(&position.average_entry_price),
      today_width = today_max,
      today = format_price(&position.unrealized_gain_today),
      today_pct_width = today_pct_max,
      today_pct = format_percent(&position.unrealized_gain_today_percent),
      total_width = total_max,
      total = format_price(&position.unrealized_gain_total),
      total_pct_width = total_pct_max,
      total_pct = format_percent(&position.unrealized_gain_total_percent),
    )
  }

  println!(
    "{empty:->pos_width$}- -{empty:->value_width$}- -\
     {empty:->today_width$}- -{empty:->total_width$}",
    empty = "",
    pos_width = position_col,
    value_width = entry_col,
    today_width = today_col,
    total_width = total_col,
  );
  println!(
    "{value:>value_width$} {currency}   \
     {base:>base_width$} {currency}   \
     {today:>today_width$} {currency} ({today_pct:>today_pct_width$}%)   \
     {total:>total_width$} {currency} ({total_pct:>total_pct_width$}%)",
    value = format_price(&total_value),
    value_width = position_col - 1 - currency.len(),
    currency = currency,
    base = format_price(&base_value),
    base_width = entry_max,
    today = format_price(&today_gain),
    today_pct = format_percent(&today_gain_pct),
    today_pct_width = today_pct_max,
    today_width = today_max,
    total_width = total_max,
    total = format_price(&total_gain),
    total_pct_width = total_pct_max,
    total_pct = format_percent(&total_gain_pct),
  );
}

/// List all currently open positions.
async fn position_list(client: Client) -> Result<(), Error> {
  let account = client
    .issue::<account::Get>(())
    .await
    .with_context(|| "failed to retrieve account information")?;

  let mut positions = client
    .issue::<positions::Get>(())
    .await
    .with_context(|| "failed to list positions")?;

  if !positions.is_empty() {
    positions.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    position_print(&positions, &account.currency);
  }
  Ok(())
}

async fn run() -> Result<(), Error> {
  let opts = Opts::from_args();
  let level = match opts.verbosity {
    0 => LevelFilter::WARN,
    1 => LevelFilter::INFO,
    2 => LevelFilter::DEBUG,
    _ => LevelFilter::TRACE,
  };

  let subscriber = FmtSubscriber::builder()
    .with_max_level(level)
    .with_timer(ChronoLocal::rfc3339())
    .finish();

  set_global_subscriber(subscriber).with_context(|| "failed to set tracing subscriber")?;

  let api_info =
    ApiInfo::from_env().with_context(|| "failed to retrieve Alpaca environment information")?;
  let client = Client::new(api_info);

  match opts.command {
    Command::Account => account(client).await,
    Command::Asset(asset) => self::asset(client, asset).await,
    Command::Events(events) => self::events(client, events).await,
    Command::Market => self::market(client).await,
    Command::Order(order) => self::order(client, order).await,
    Command::Position(position) => self::position(client, position).await,
  }
}

fn main() {
  let mut rt = Runtime::new().unwrap();
  let exit_code = rt
    .block_on(run())
    .map(|_| 0)
    .map_err(|e| {
      eprint!("{}", e);
      e.chain().skip(1).for_each(|cause| eprint!(": {}", cause));
      eprintln!();
    })
    .unwrap_or(1);
  // We exit the process the hard way next, so make sure to flush
  // buffered content.
  let _ = stdout().flush();
  exit(exit_code)
}
