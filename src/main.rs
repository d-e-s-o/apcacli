// Copyright (C) 2019 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::max;
use std::io::stdout;
use std::io::Write;
use std::process::exit;
use std::str::FromStr;

use apca::api::v2::account;
use apca::api::v2::asset;
use apca::api::v2::order;
use apca::api::v2::orders;
use apca::api::v2::position;
use apca::api::v2::positions;
use apca::ApiInfo;
use apca::Client;

use futures::future::Either;
use futures::future::Future;
use futures::future::err;
use futures::future::ok;
use futures::stream::futures_unordered;
use futures_ext::StreamExt;

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
  /// Perform various position related functions.
  #[structopt(name = "position")]
  Position(Position),
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
    /// Create an order that is only valid for today.
    #[structopt(long = "today")]
    today: bool,
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
  type Err = ParseError;

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
  type Err = ParseError;

  fn from_str(id: &str) -> Result<Self, Self::Err> {
    Ok(OrderId(order::Id(Uuid::parse_str(id)?)))
  }
}


#[derive(Debug, StructOpt)]
enum Position {
  /// List all open positions.
  #[structopt(name = "list")]
  List,
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
fn account(client: Client) -> Result<Box<dyn Future<Item = (), Error = ()>>, ()> {
  let fut = client
    .issue::<account::Get>(())
    .map_err(|e| eprintln!("failed to issue GET request to account endpoint: {}", e))?
    .map_err(|e| eprintln!("failed to retrieve account information: {}", e))
    .and_then(|account| {
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
      ok(())
    });

  Ok(Box::new(fut))
}


/// The handler for the 'order' command.
fn order(
  client: Client,
  order: Order,
) -> Result<Box<dyn Future<Item = (), Error = ()>>, ()> {
  match order {
    Order::Submit {
      side,
      symbol,
      quantity,
      limit_price,
      stop_price,
      extended_hours,
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
        extended_hours,
      };

      let fut = client
        .issue::<order::Post>(request)
        .map_err(|e| eprintln!("failed to issue POST request to order endpoint: {}", e))?
        .map_err(|e| eprintln!("failed to submit order: {}", e))
        .and_then(|order| {
          println!("{}", order.id.to_hyphenated_ref());
          ok(())
        });

      Ok(Box::new(fut))
    },
    Order::Cancel{cancel} => order_cancel(client, cancel),
    Order::List => order_list(client),
  }
}


/// Cancel an open order.
fn order_cancel(
  client: Client,
  cancel: CancelOrder,
) -> Result<Box<dyn Future<Item = (), Error = ()>>, ()> {
  match cancel {
    CancelOrder::ById(id) => {
      let fut = client
        .issue::<order::Delete>(id.0)
        .map_err(|e| eprintln!("failed to issue DELETE request to order endpoint: {}", e))?
        .map_err(|e| eprintln!("failed to cancel order: {}", e));
      Ok(Box::new(fut))
    },
    CancelOrder::All => {
      let request = orders::OrdersReq { limit: 500 };
      let orders = client
        .issue::<orders::Get>(request)
        .map_err(|e| eprintln!("failed to issue GET request to orders endpoint: {}", e))?
        .map_err(|e| eprintln!("failed to list orders: {}", e));

      let cancels = orders
        .and_then(move |orders| {
          let iter = orders
            .iter()
            .map(|order| {
              let result = client
                .issue::<order::Delete>(order.id)
                .map_err(|e| {
                  let id = order.id.to_hyphenated_ref();
                  eprintln!("failed to issue DELETE request for order {}: {}", id, e)
                })
                .map(|req| {
                  req
                    .map_err(|e| eprintln!("failed to cancel order: {}", e))
                });

              // At this point we have a Result<Future<(), ()>, ()> but
              // really what we want is a Future<(), ()>. So flatten the
              // result here by merging the error (we want to preserve the
              // fact that we encountered an error, but we already
              // printed the error itself).
              match result {
                Ok(req) => Either::A(req),
                Err(e) => Either::B(err(e)),
              }
            });

          futures_unordered(iter)
            .fold_results(Ok(()), |acc, res| acc.and(res))
        });

      Ok(Box::new(cancels))
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
fn order_list(client: Client) -> Result<Box<dyn Future<Item = (), Error = ()>>, ()> {
  let account = client
    .issue::<account::Get>(())
    .map_err(|e| eprintln!("failed to issue GET request to account endpoint: {}", e))?
    .map_err(|e| eprintln!("failed to retrieve account information: {}", e));

  let request = orders::OrdersReq { limit: 500 };
  let orders = client
    .issue::<orders::Get>(request)
    .map_err(|e| eprintln!("failed to issue GET request to orders endpoint: {}", e))?
    .map_err(|e| eprintln!("failed to list orders: {}", e));

  let fut = account.join(orders).and_then(|(account, mut orders)| {
    let currency = account.currency;

    orders.sort_by(|a, b| a.symbol.cmp(&b.symbol));

    let qty_max = max_width(&orders, |p| format_quantity(&p.quantity).len());
    let sym_max = max_width(&orders, |p| p.symbol.len());

    for order in orders {
      let side = match order.side {
        order::Side::Buy => "buy",
        order::Side::Sell => "sell",
      };
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
        side = side,
        qty_width = qty_max,
        qty = format!("{:.0}", order.quantity),
        sym_width = sym_max,
        sym = order.symbol,
        price = price,
      )
    }
    ok(())
  });

  Ok(Box::new(fut))
}


/// The handler for the 'position' command.
fn position(
  client: Client,
  position: Position,
) -> Result<Box<dyn Future<Item = (), Error = ()>>, ()> {
  match position {
    Position::List => position_list(client),
  }
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
fn position_list(client: Client) -> Result<Box<dyn Future<Item = (), Error = ()>>, ()> {
  let account = client
    .issue::<account::Get>(())
    .map_err(|e| eprintln!("failed to issue GET request to account endpoint: {}", e))?
    .map_err(|e| eprintln!("failed to retrieve account information: {}", e));

  let positions = client
    .issue::<positions::Get>(())
    .map_err(|e| eprintln!("failed to issue GET request to positions endpoint: {}", e))?
    .map_err(|e| eprintln!("failed to list positions: {}", e));

  let fut = account.join(positions).and_then(|(account, mut positions)| {
    if !positions.is_empty() {
      positions.sort_by(|a, b| a.symbol.cmp(&b.symbol));
      position_print(&positions, &account.currency);
    }
    ok(())
  });

  Ok(Box::new(fut))
}

fn run() -> Result<(), ()> {
  let opts = Opts::from_args();
  let level = match opts.verbosity {
    0 => LevelFilter::Warn,
    1 => LevelFilter::Info,
    2 => LevelFilter::Debug,
    _ => LevelFilter::Trace,
  };

  let _ = SimpleLogger::init(level, Config::default());
  let api_info = ApiInfo::from_env().map_err(|e| {
    eprintln!("failed to retrieve Alpaca environment information: {}", e)
  })?;
  let client = Client::new(api_info).map_err(|e| {
    eprintln!("failed to create Alpaca client: {}", e)
  })?;

  let future = match opts.command {
    Command::Account => account(client),
    Command::Order(order) => self::order(client, order),
    Command::Position(position) => self::position(client, position),
  }?;

  block_on_all(future)
}

fn main() {
  let exit_code = run().map(|_| 0).unwrap_or(1);
  // We exit the process the hard way next, so make sure to flush
  // buffered content.
  let _ = stdout().flush();
  exit(exit_code)
}
