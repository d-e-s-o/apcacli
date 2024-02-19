// Copyright (C) 2023 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::let_and_return, clippy::let_unit_value)]

use std::borrow::Cow;
use std::collections::HashSet;
use std::env::var_os;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::stdout;
use std::io::Write;
use std::process::exit;

use apca::api::v2::order;
use apca::api::v2::orders;
use apca::api::v2::position;
use apca::api::v2::positions;
use apca::ApiInfo;
use apca::Client;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use anyhow::Context;
use anyhow::Result;

use num_decimal::Num;

use clap::ArgAction;
use clap::Parser;

use tokio::runtime::Builder;

use tracing::info;
use tracing::span;
use tracing::subscriber::set_global_default as set_global_subscriber;
use tracing::Level;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::SystemTime;
use tracing_subscriber::FmtSubscriber;


/// The minimum markup for the limit price, expressed in basis points
/// (i.e., 100th of a percent).
const LIMIT_ORDER_MARKUP: usize = 10;
/// The minimum markup for the stop price, expressed in basis points
/// (i.e., 100th of a percent).
const STOP_ORDER_MARKUP: usize = 100;


/// A program for ensuring that positions in Alpaca have accurate
/// stop-loss orders.
#[derive(Debug, Parser)]
struct Args {
  /// Symbols of positions to set/change stop orders for.
  positions: Vec<String>,
  /// The apcacli command to use in printed commands.
  #[clap(long)]
  apcacli: Option<OsString>,
  /// Set the stop price at this many percentage points gained.
  #[clap(short, long, name = "PERCENT")]
  stop_percent: Option<usize>,
  /// The minimum value of a position required for stop-loss order
  /// creation.
  #[clap(short, long)]
  min_value: Option<usize>,
  /// The minimum gain a position needs to have for it to be considered
  /// for stop-loss order creation.
  #[clap(short = 'g', long, default_value = "5")]
  min_gain_percent: usize,
  /// Increase verbosity (can be supplied multiple times).
  #[clap(short = 'v', long = "verbose", global = true, action = ArgAction::Count)]
  verbosity: u8,
}


/// Check if the given order is opposing the given position.
fn opposing_sides(position: &position::Position, order: &order::Order) -> bool {
  matches!(
    (position.side, order.side),
    (position::Side::Long, order::Side::Sell) | (position::Side::Short, order::Side::Buy)
  )
}


/// Evaluate the provided position against the given list of orders.
fn evaluate_position(
  args: &Args,
  position: &position::Position,
  orders: &[order::Order],
) -> Result<()> {
  let mut found = false;

  let cli = args
    .apcacli
    .as_deref()
    .map(Cow::Borrowed)
    .or_else(|| var_os("APCACLI").map(Cow::Owned))
    .unwrap_or_else(|| Cow::Borrowed(OsStr::new("apcacli")));
  let cli = cli.to_string_lossy();

  let limit_factor = Num::new(10_000 + LIMIT_ORDER_MARKUP, 10_000);
  let stop_factor = Num::new(
    10_000
      + args
        .stop_percent
        .map(|x| x * 100)
        .unwrap_or(STOP_ORDER_MARKUP),
    10_000,
  );

  // TODO: For true penny stocks it may be possible that we end
  //       up with a limit price that is equal to the purchase
  //       price, I guess, because we round to two post-decimal
  //       positions.
  let desired_limit = (&position.average_entry_price * limit_factor).round_with(2);
  let desired_stop = (&position.average_entry_price * stop_factor).round_with(2);

  for order in orders {
    if order.symbol == position.symbol
      && opposing_sides(position, order)
      && order.stop_price.is_some()
    {
      ensure!(!found, "found multiple stop-loss orders");
      ensure!(
        order.time_in_force == order::TimeInForce::UntilCanceled,
        anyhow!(
          "opposing order {} is not valid-until-canceled",
          order.id.as_hyphenated()
        )
      );

      let quantity = match &order.amount {
        order::Amount::Quantity { quantity } => quantity,
        order::Amount::Notional { .. } => bail!("notional orders are currently unsupported"),
      };

      found = true;

      let limit = order.limit_price.clone().unwrap_or_default();
      let stop = order.stop_price.clone().unwrap_or_default();

      if quantity != &position.quantity || limit < desired_limit || stop < desired_stop {
        ensure!(
          order.side == order::Side::Sell,
          "only long positions are currently supported",
        );

        println!(
          "{sym}:\n{cli} order change {id} --quantity {qty} --limit-price {limit} --stop-price {stop}",
          sym = position.symbol,
          cli = cli,
          id = order.id.as_hyphenated(),
          qty = position.quantity,
          limit = desired_limit,
          stop = desired_stop,
        );
      } else {
        info!(
          "order {} is satisfying stop-loss order",
          order.id.as_hyphenated()
        )
      }
    }
  }

  if !found {
    let total_gain = position
      .unrealized_gain_total_percent
      .clone()
      .unwrap_or_default()
      * 100;
    if total_gain < Num::from(args.min_gain_percent) {
      info!(
        "total gain ({:.2}%) is below {}%",
        total_gain, args.min_gain_percent
      );
      return Ok(())
    }

    if let Some(min_value) = args.min_value {
      let total_value = &position.quantity * position.current_price.clone().unwrap_or_default();
      if total_value < Num::from(min_value) {
        info!(
          "total value ({}) is still less than {:.2}",
          total_value, min_value
        );
        return Ok(())
      }
    }

    println!(
      "{sym}:\n{cli} order submit sell {sym} --quantity {qty} --limit-price {limit} --stop-price {stop}",
          sym = position.symbol,
          cli = cli,
          qty = position.quantity,
          limit = desired_limit.round_with(2),
          stop = desired_stop.round_with(2),
    )
  }
  Ok(())
}


/// Evaluate the given position against the given orders.
fn evaluate_positions_and_orders(
  args: &Args,
  positions: &[position::Position],
  orders: &[order::Order],
) -> Result<()> {
  let symbols = if !args.positions.is_empty() {
    Some(args.positions.iter().cloned().collect::<HashSet<_>>())
  } else {
    None
  };

  for position in positions {
    let evaluate = symbols
      .as_ref()
      .map_or(true, |symbols| symbols.contains(&position.symbol));
    if !evaluate {
      continue
    }

    let span = span!(Level::INFO, "evaluate", symbol = display(&position.symbol));
    let _enter = span.enter();
    let () = evaluate_position(args, position, orders)
      .with_context(|| format!("failed to evaluate {} position", position.symbol))?;
  }
  Ok(())
}


async fn run() -> Result<()> {
  let args = Args::parse();
  let level = match args.verbosity {
    0 => LevelFilter::WARN,
    1 => LevelFilter::INFO,
    2 => LevelFilter::DEBUG,
    _ => LevelFilter::TRACE,
  };

  let subscriber = FmtSubscriber::builder()
    .with_max_level(level)
    .with_timer(SystemTime)
    .finish();

  let () = set_global_subscriber(subscriber).with_context(|| "failed to set tracing subscriber")?;

  let api_info =
    ApiInfo::from_env().with_context(|| "failed to retrieve Alpaca environment information")?;
  let client = Client::new(api_info);

  // TODO: We may want to retrieve orders and positions concurrently.
  let positions = client
    .issue::<positions::Get>(&())
    .await
    .with_context(|| "failed to retrieve position information")?;

  let request = orders::OrdersReq {
    symbols: Vec::new(),
    status: orders::Status::Open,
    limit: Some(500),
    // It shouldn't be necessary for us to work with nested orders here.
    nested: false,
  };
  let orders = client
    .issue::<orders::Get>(&request)
    .await
    .with_context(|| "failed to retrieve order information")?;

  evaluate_positions_and_orders(&args, &positions, &orders)
}


fn main() {
  let rt = Builder::new_current_thread().enable_io().build().unwrap();
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
