// Copyright (C) 2019-2021 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

#![type_length_limit = "536870912"]
#![allow(clippy::large_enum_variant, clippy::let_and_return)]

mod args;

use std::borrow::Cow;
use std::cmp::max;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::io::stdout;
use std::io::Write;
use std::ops::Deref as _;
use std::process::exit;

use apca::api::v2::account;
use apca::api::v2::account_activities;
use apca::api::v2::account_config;
use apca::api::v2::asset;
use apca::api::v2::assets;
use apca::api::v2::clock;
use apca::api::v2::events;
use apca::api::v2::order;
use apca::api::v2::orders;
use apca::api::v2::position;
use apca::api::v2::positions;
use apca::data::v1::bars;
use apca::event;
use apca::ApiInfo;
use apca::Client;

use anyhow::anyhow;
use anyhow::Context;
use anyhow::Error;
use anyhow::Result;

use chrono::offset::Local;
use chrono::offset::Utc;
use chrono::DateTime;

use futures::future::ready;
use futures::future::TryFutureExt;
use futures::join;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;

use num_decimal::Num;

use serde_json::value::RawValue;

use structopt::StructOpt;

use tokio::runtime::Builder;

use tracing::subscriber::set_global_default as set_global_subscriber;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::FmtSubscriber;

use yansi::Paint;

use crate::args::Account;
use crate::args::Activity;
use crate::args::Args;
use crate::args::Asset;
use crate::args::CancelOrder;
use crate::args::ChangeOrder;
use crate::args::Command;
use crate::args::Config;
use crate::args::ConfigSet;
use crate::args::EventType;
use crate::args::Events;
use crate::args::Order;
use crate::args::OrderId;
use crate::args::Position;
use crate::args::Side;
use crate::args::SubmitOrder;
use crate::args::Symbol;


/// The string type we use on many occasions.
type Str = Cow<'static, str>;


// A replacement of the standard println!() macro that does not panic
// when encountering an EPIPE.
macro_rules! println {
  ($($arg:tt)*) => {
    match writeln!(::std::io::stdout(), $($arg)*) {
      Ok(..) => (),
      Err(err) if err.kind() == ::std::io::ErrorKind::BrokenPipe => (),
      Err(err) => panic!("failed printing to stdout: {}", err),
    }
  };
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
    account::Status::Unknown => "unknown",
  }
  .to_string()
}


/// The handler for the 'account' command.
async fn account(client: Client, account: Account) -> Result<()> {
  match account {
    Account::Get => account_get(client).await,
    Account::Activity(activity) => account_activity(client, activity).await,
    Account::Config(config) => account_config(client, config).await,
  }
}

/// Print information about the account.
async fn account_get(client: Client) -> Result<()> {
  let account = client
    .issue::<account::Get>(&())
    .await
    .with_context(|| "failed to retrieve account information")?;

  println!(
    r#"account:
  id:                 {id}
  status:             {status}
  buying power:       {buying_power}
  cash:               {cash}
  long value:         {value_long}
  short value:        {value_short}
  equity:             {equity}
  last equity:        {last_equity}
  margin multiplier:  {multiplier}
  initial margin:     {initial_margin}
  maintenance margin: {maintenance_margin}
  day trade count:    {day_trade_count}
  day trader:         {day_trader}
  shorting enabled:   {shorting_enabled}
  trading suspended:  {trading_suspended}
  trading blocked:    {trading_blocked}
  transfers blocked:  {transfers_blocked}
  account blocked:    {account_blocked}"#,
    id = account.id.to_hyphenated_ref(),
    status = format_account_status(account.status),
    buying_power = format_price(&account.buying_power, &account.currency),
    cash = format_price(&account.cash, &account.currency),
    value_long = format_price(&account.market_value_long, &account.currency),
    value_short = format_price(&account.market_value_short, &account.currency),
    equity = format_price(&account.equity, &account.currency),
    last_equity = format_price(&account.last_equity, &account.currency),
    multiplier = account.multiplier,
    initial_margin = format_price(&account.initial_margin, &account.currency),
    maintenance_margin = format_price(&account.maintenance_margin, &account.currency),
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


/// The handler for the 'account activity' command.
async fn account_activity(client: Client, activity: Activity) -> Result<()> {
  match activity {
    Activity::Get => account_activity_get(client).await,
  }
}

fn format_activity_side(side: account_activities::Side) -> &'static str {
  match side {
    account_activities::Side::Buy => "buy",
    account_activities::Side::Sell => "sell",
    account_activities::Side::ShortSell => "short sell",
  }
}

fn format_activity_type(side: account_activities::ActivityType) -> &'static str {
  match side {
    account_activities::ActivityType::Fill => unreachable!(),
    account_activities::ActivityType::Transaction => "transaction",
    account_activities::ActivityType::Miscellaneous => "miscellaneous",
    account_activities::ActivityType::AcatsInOutCash
    | account_activities::ActivityType::AcatsInOutSecurities => "transfer",
    account_activities::ActivityType::CashDeposit
    | account_activities::ActivityType::CashWithdrawal => "cash",
    account_activities::ActivityType::CapitalGainLongTerm
    | account_activities::ActivityType::CapitalGainShortTerm => "capital gains",
    account_activities::ActivityType::Dividend
    | account_activities::ActivityType::DividendFee
    | account_activities::ActivityType::DividendTaxExtempt
    | account_activities::ActivityType::DividendReturnOfCapital => "dividend",
    account_activities::ActivityType::DividendAdjusted
    | account_activities::ActivityType::DividendAdjustedNraWithheld
    | account_activities::ActivityType::DividendAdjustedTefraWithheld => "dividend adjusted",
    account_activities::ActivityType::Interest => "interest",
    account_activities::ActivityType::InterestAdjustedNraWithheld
    | account_activities::ActivityType::InterestAdjustedTefraWithheld => "interested adjusted",
    account_activities::ActivityType::JournalEntry
    | account_activities::ActivityType::JournalEntryCash
    | account_activities::ActivityType::JournalEntryStock => unimplemented!(),
    account_activities::ActivityType::Acquisition => "acquisition",
    account_activities::ActivityType::NameChange => "name change",
    account_activities::ActivityType::OptionAssignment => "option assigned",
    account_activities::ActivityType::OptionExpiration => "option expired",
    account_activities::ActivityType::OptionExercise => "option exercised",
    account_activities::ActivityType::PassThruCharge => "pass-through charge",
    account_activities::ActivityType::PassThruRebate => "pass-through rebate",
    account_activities::ActivityType::Reorg => "reorganization",
    account_activities::ActivityType::SymbolChange => "symbol change",
    account_activities::ActivityType::StockSpinoff => "stock spin-off",
    account_activities::ActivityType::StockSplit => "stock split",
    account_activities::ActivityType::Fee => "regulatory fee",
    account_activities::ActivityType::Unknown => unreachable!(),
  }
}


/// Sort a vector of `Activity` objects in descending order of their
/// time stamps.
fn sort_account_activity(activities: &mut Vec<account_activities::Activity>) {
  activities.sort_by(|act1, act2| {
    let ordering = match act1 {
      account_activities::Activity::Trade(trade1) => match act2 {
        account_activities::Activity::Trade(trade2) => {
          trade1.transaction_time.cmp(&trade2.transaction_time)
        },
        account_activities::Activity::NonTrade(non_trade) => {
          trade1.transaction_time.cmp(&non_trade.date)
        },
      },
      account_activities::Activity::NonTrade(non_trade1) => match act2 {
        account_activities::Activity::Trade(trade) => non_trade1.date.cmp(&trade.transaction_time),
        account_activities::Activity::NonTrade(non_trade2) => non_trade1.date.cmp(&non_trade2.date),
      },
    };
    ordering.reverse()
  });
}


/// Retrieve account activity.
async fn account_activity_get(client: Client) -> Result<()> {
  let request = account_activities::ActivityReq::default();
  let currency = client.issue::<account::Get>(&());
  let activity = client.issue::<account_activities::Get>(&request);

  let (currency, activity) = join!(currency, activity);
  let currency = currency
    .with_context(|| "failed to retrieve account information")?
    .currency;
  let mut activities = activity.with_context(|| "failed to retrieve account activity")?;
  sort_account_activity(&mut activities);

  for activity in activities {
    match activity {
      account_activities::Activity::Trade(trade) => {
        println!(
          r#"{time}  {side} {qty} {sym} @ {price} = {total}"#,
          time = format_local_time_short(trade.transaction_time),
          side = format_activity_side(trade.side),
          qty = trade.quantity,
          sym = trade.symbol,
          price = format_price(&trade.price, &currency),
          total = format_price(&(trade.price * trade.quantity as i32), &currency),
        );
      },
      account_activities::Activity::NonTrade(non_trade) => {
        println!(
          r#"{date:19}  {activity} {amount}"#,
          date = format_date(non_trade.date),
          activity = format_activity_type(non_trade.type_),
          amount = format_price(&non_trade.net_amount, &currency),
        );
      },
    }
  }
  Ok(())
}


/// Retrieve or modify the account configuration.
async fn account_config(client: Client, config: Config) -> Result<()> {
  match config {
    Config::Get => account_config_get(client).await,
    Config::Set(set) => account_config_set(client, set).await,
  }
}


/// Format an account status.
fn format_trade_confirmation(confirmation: account_config::TradeConfirmation) -> &'static str {
  match confirmation {
    account_config::TradeConfirmation::Email => "e-mail",
    account_config::TradeConfirmation::None => "none",
  }
}

/// Retrieve the account configuration.
async fn account_config_get(client: Client) -> Result<()> {
  let config = client
    .issue::<account_config::Get>(&())
    .await
    .with_context(|| "failed to retrieve account configuration")?;

  println!(
    r#"account configuration:
  trade confirmation:  {trade_confirmation}
  trading suspended:   {trading_suspended}
  shorting enabled:    {shorting}"#,
    trade_confirmation = format_trade_confirmation(config.trade_confirmation),
    trading_suspended = config.trading_suspended,
    shorting = !config.no_shorting,
  );
  Ok(())
}

/// Modify the account configuration.
async fn account_config_set(client: Client, set: ConfigSet) -> Result<()> {
  let mut config = client
    .issue::<account_config::Get>(&())
    .await
    .with_context(|| "failed to retrieve account configuration")?;

  config.trade_confirmation = if set.confirm_email {
    debug_assert!(!set.no_confirm_email);
    account_config::TradeConfirmation::Email
  } else if set.no_confirm_email {
    debug_assert!(!set.confirm_email);
    account_config::TradeConfirmation::None
  } else {
    config.trade_confirmation
  };

  config.trading_suspended = if set.trading_suspended {
    debug_assert!(!set.no_trading_suspended);
    true
  } else if set.no_trading_suspended {
    debug_assert!(!set.trading_suspended);
    false
  } else {
    config.trading_suspended
  };

  config.no_shorting = if set.shorting {
    debug_assert!(!set.no_shorting);
    false
  } else if set.no_shorting {
    debug_assert!(!set.shorting);
    true
  } else {
    config.no_shorting
  };

  let _ = client
    .issue::<account_config::Patch>(&config)
    .await
    .with_context(|| "failed to update account configuration")?;
  Ok(())
}


/// The handler for the 'asset' command.
async fn asset(client: Client, asset: Asset) -> Result<()> {
  match asset {
    Asset::Get { symbol } => asset_get(client, symbol).await,
    Asset::List => asset_list(client).await,
  }
}

/// Print information about the asset with the given symbol.
async fn asset_get(client: Client, symbol: Symbol) -> Result<()> {
  let asset = client
    .issue::<asset::Get>(&symbol.0)
    .await
    .with_context(|| format!("failed to retrieve asset information for {}", symbol.0))?;

  println!(
    r#"{sym}:
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
async fn asset_list(client: Client) -> Result<()> {
  let request = assets::AssetsReq {
    status: asset::Status::Active,
    class: asset::Class::UsEquity,
  };
  let mut assets = client
    .issue::<assets::Get>(&request)
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

/// Format a date time as per RFC 2822, after converting to local date
/// time.
fn format_local_time(time: DateTime<Utc>) -> Str {
  DateTime::<Local>::from(time).to_rfc2822().into()
}

/// Format a date time, after converting to local date time.
fn format_local_time_short(time: DateTime<Utc>) -> Str {
  DateTime::<Local>::from(time)
    .format("%Y-%m-%d %H:%M:%S")
    .to_string()
    .into()
}

/// Format a date time as a date.
fn format_date(time: DateTime<Utc>) -> Str {
  time.date().format("%Y-%m-%d").to_string().into()
}


/// A helper for receiving raw JSON account updates.
enum RawAccountUpdates {}

impl event::EventStream for RawAccountUpdates {
  type Event = Box<RawValue>;

  fn stream() -> event::StreamType {
    event::StreamType::AccountUpdates
  }
}


async fn stream_account_updates(client: Client, json: bool) -> Result<()> {
  if json {
    event::stream::<RawAccountUpdates>(client.api_info())
      .await
      .with_context(|| "failed to subscribe to account updates")?
      .map_err(Error::from)
      .try_for_each(|result| {
        async {
          let update = result.unwrap();
          // TODO: We should honor the write result.
          let _result = stdout().write(update.get().as_bytes());
          Ok(())
        }
      })
      .await?;
  } else {
    client
      .subscribe::<events::AccountUpdates>()
      .await
      .with_context(|| "failed to subscribe to account updates")?
      .map_err(Error::from)
      .try_for_each(|result| async {
        let update = result.unwrap();
        println!(
          r#"account update:
  status:        {status}
  created at:    {created}
  updated at:    {updated}
  deleted at:    {deleted}
  cash:          {cash}
  withdrawable:  {withdrawable}
"#,
          status = update.status,
          created = update
            .created_at
            .map(format_local_time)
            .unwrap_or_else(|| "N/A".into()),
          updated = update
            .updated_at
            .map(format_local_time)
            .unwrap_or_else(|| "N/A".into()),
          deleted = update
            .deleted_at
            .map(format_local_time)
            .unwrap_or_else(|| "N/A".into()),
          cash = format_price(&update.cash, &update.currency),
          withdrawable = format_price(&update.withdrawable_cash, &update.currency),
        );
        Ok(())
      })
      .await?;
  }

  Ok(())
}


fn format_trade_status(status: events::TradeStatus) -> &'static str {
  match status {
    events::TradeStatus::New => "new",
    events::TradeStatus::Replaced => "replaced",
    events::TradeStatus::ReplaceRejected => "replace rejected",
    events::TradeStatus::PartialFill => "partially filled",
    events::TradeStatus::Filled => "filled",
    events::TradeStatus::DoneForDay => "done for day",
    events::TradeStatus::Canceled => "canceled",
    events::TradeStatus::CancelRejected => "cancel rejected",
    events::TradeStatus::Expired => "expired",
    events::TradeStatus::PendingCancel => "pending cancel",
    events::TradeStatus::Stopped => "stopped",
    events::TradeStatus::Rejected => "rejected",
    events::TradeStatus::Suspended => "suspended",
    events::TradeStatus::PendingNew => "pending new",
    events::TradeStatus::PendingReplace => "pending replace",
    events::TradeStatus::Calculated => "calculated",
    events::TradeStatus::Unknown => "unknown",
  }
}

fn format_order_status(status: order::Status) -> &'static str {
  match status {
    order::Status::New => "new",
    order::Status::Replaced => "replaced",
    order::Status::PartiallyFilled => "partially filled",
    order::Status::Filled => "filled",
    order::Status::DoneForDay => "done for day",
    order::Status::Canceled => "canceled",
    order::Status::Expired => "expired",
    order::Status::Accepted => "accepted",
    order::Status::PendingNew => "pending new",
    order::Status::AcceptedForBidding => "accepted for bidding",
    order::Status::PendingCancel => "pending cancel",
    order::Status::PendingReplace => "pending replace",
    order::Status::Stopped => "stopped",
    order::Status::Rejected => "rejected",
    order::Status::Suspended => "suspended",
    order::Status::Calculated => "calculated",
    order::Status::Held => "held",
    order::Status::Unknown => "unknown",
  }
}

fn format_order_type(type_: order::Type) -> &'static str {
  match type_ {
    order::Type::Market => "market",
    order::Type::Limit => "limit",
    order::Type::Stop => "stop",
    order::Type::StopLimit => "stop-limit",
    order::Type::TrailingStop => "trailing-stop",
  }
}

fn format_order_side(side: order::Side) -> &'static str {
  match side {
    order::Side::Buy => "buy",
    order::Side::Sell => "sell",
  }
}

fn format_time_in_force(time_in_force: order::TimeInForce) -> &'static str {
  match time_in_force {
    order::TimeInForce::Day => "today",
    order::TimeInForce::UntilCanceled => "canceled",
    order::TimeInForce::UntilMarketOpen => "market open",
    order::TimeInForce::UntilMarketClose => "market close",
  }
}

/// Format a `TimeInForce` value as a short three letter acronym.
fn format_time_in_force_short(time_in_force: order::TimeInForce) -> &'static str {
  match time_in_force {
    order::TimeInForce::Day => "day",
    order::TimeInForce::UntilCanceled => "gtc",
    order::TimeInForce::UntilMarketOpen => "opn",
    order::TimeInForce::UntilMarketClose => "cls",
  }
}


/// A helper for receiving raw JSON trade updates.
enum RawTradeUpdates {}

impl event::EventStream for RawTradeUpdates {
  type Event = Box<RawValue>;

  fn stream() -> event::StreamType {
    event::StreamType::TradeUpdates
  }
}


async fn stream_trade_updates(client: Client, json: bool) -> Result<()> {
  if json {
    event::stream::<RawTradeUpdates>(client.api_info())
      .await
      .with_context(|| "failed to subscribe to trade updates")?
      .map_err(Error::from)
      .try_for_each(|result| async {
        let update = result.unwrap();
        // TODO: We should honor the write result.
        let _result = stdout().write(update.get().as_bytes());
        Ok(())
      })
      .await?;
  } else {
    client
      .subscribe::<events::TradeUpdates>()
      .await
      .with_context(|| "failed to subscribe to trade updates")?
      .map_err(Error::from)
      .try_for_each(|result| async {
        let update = result.unwrap();
        println!(
          r#"{symbol} {status}:
  order id:       {id}
  status:         {order_status}
  type:           {type_}
  side:           {side}
  time-in-force:  {time_in_force}
  quantity:       {quantity}
  filled:         {filled}
"#,
          symbol = update.order.symbol,
          status = format_trade_status(update.event),
          id = update.order.id.to_hyphenated_ref(),
          order_status = format_order_status(update.order.status),
          type_ = format_order_type(update.order.type_),
          side = format_order_side(update.order.side),
          time_in_force = format_time_in_force(update.order.time_in_force),
          quantity = update.order.quantity,
          filled = update.order.filled_quantity,
        );
        Ok(())
      })
      .await?;
  }

  Ok(())
}

async fn events(client: Client, events: Events) -> Result<()> {
  match events.event {
    EventType::Account => stream_account_updates(client, events.json).await,
    EventType::Trades => stream_trade_updates(client, events.json).await,
  }
}

/// Print the current market status.
async fn market(client: Client) -> Result<()> {
  let clock = client
    .issue::<clock::Get>(&())
    .await
    .with_context(|| "failed to retrieve market clock")?;

  println!(
    r#"market:
  open:         {open}
  current time: {current}
  next open:    {next_open}
  next close:   {next_close}"#,
    open = clock.open,
    current = format_local_time(clock.current),
    next_open = format_local_time(clock.next_open),
    next_close = format_local_time(clock.next_close),
  );
  Ok(())
}


/// Convert a certain monetary value into the maximum number of shares
/// purchasable (i.e., a quantity).
async fn value_to_quantity(
  client: &Client,
  symbol: &str,
  value: &Num,
  price: Option<Num>,
) -> Result<u64> {
  let price = match price {
    Some(price) => Ok(price),
    None => {
      let request = bars::BarReq {
        symbol: symbol.to_string(),
        limit: 1,
        end: None,
      };

      let mut response = client
        .issue::<bars::Get>(&(bars::TimeFrame::OneMinute, request))
        .await
        .with_context(|| format!("failed to retrieve current market value of {}", symbol))?;

      if let Some(bars) = response.get_mut(symbol) {
        if bars.len() == 1 {
          // We use the last close price as our reference to infer the
          // number of stocks to purchase from.
          let bars::Bar { close, .. } = bars.remove(0);
          Ok(close)
        } else {
          Err(anyhow!(
            "market data response for {} contained unexpected number of bars: {}",
            symbol,
            bars.len()
          ))
        }
      } else {
        Err(anyhow!(
          "market data for {} not present in response",
          symbol
        ))
      }
    },
  }?;

  // We `round` as opposed to `trunc` to have a little less bias in
  // there and in an attempt to treat short orders equally.
  let amount = (value / price).round();
  let amount = amount
    .to_i64()
    .ok_or_else(|| anyhow!("calculated amount {} is not a valid 64 bit integer"))?;
  let amount = amount
    .try_into()
    .with_context(|| format!("amount {} is not unsigned", amount))?;
  Ok(amount)
}


/// The handler for the 'order' command.
async fn order(client: Client, order: Order) -> Result<()> {
  match order {
    Order::Submit(submit) => order_submit(client, submit).await,
    Order::Change(change) => order_change(client, change).await,
    Order::Cancel { cancel } => order_cancel(client, cancel).await,
    Order::Get { id } => order_get(client, id).await,
    Order::List { closed } => order_list(client, closed).await,
  }
}


/// Determine the type of an order by looking at the limit and stop
/// prices, if any.
fn determine_order_type(limit_price: &Option<Num>, stop_price: &Option<Num>) -> order::Type {
  match (limit_price.is_some(), stop_price.is_some()) {
    (true, true) => order::Type::StopLimit,
    (true, false) => order::Type::Limit,
    (false, true) => order::Type::Stop,
    (false, false) => order::Type::Market,
  }
}


/// Submit an order.
async fn order_submit(client: Client, submit: SubmitOrder) -> Result<()> {
  let SubmitOrder {
    side,
    symbol,
    quantity,
    value,
    limit_price,
    stop_price,
    take_profit_price,
    extended_hours,
    time_in_force,
  } = submit;

  let class = if take_profit_price.is_some() {
    order::Class::OneTriggersOther
  } else {
    order::Class::Simple
  };

  let side = match side {
    Side::Buy => order::Side::Buy,
    Side::Sell => order::Side::Sell,
  };

  let quantity = match (quantity, value) {
    (Some(quantity), None) => quantity,
    (None, Some(value)) => value_to_quantity(&client, &symbol, &value, limit_price.clone())
      .await
      .with_context(|| "unable to convert value to quantity")?,
    // Other combinations should never happen as ensured by `clap`.
    _ => unreachable!(),
  };

  let type_ = determine_order_type(&limit_price, &stop_price);
  let take_profit = take_profit_price.map(order::TakeProfit::Limit);
  let time_in_force = time_in_force.to_time_in_force();

  // TODO: We should probably support other forms of specifying
  //       the symbol.
  let request = order::OrderReqInit {
    class,
    type_,
    time_in_force,
    limit_price,
    stop_price,
    take_profit,
    extended_hours,
    ..Default::default()
  }
  .init(symbol, side, quantity);

  let order = client
    .issue::<order::Post>(&request)
    .await
    .with_context(|| "failed to submit order")?;

  println!("{}", order.id.to_hyphenated_ref());
  for leg in order.legs {
    println!("  {}", leg.id.to_hyphenated_ref());
  }
  Ok(())
}


/// Change an order.
async fn order_change(client: Client, change: ChangeOrder) -> Result<()> {
  let ChangeOrder {
    id,
    quantity,
    value,
    limit_price,
    stop_price,
    time_in_force,
  } = change;

  let mut order = client
    .issue::<order::Get>(&id.0)
    .await
    .with_context(|| format!("failed to retrieve order {}", id.0.to_hyphenated_ref()))?;

  let time_in_force = time_in_force
    .map(|x| x.to_time_in_force())
    .unwrap_or(order.time_in_force);
  let limit_price = limit_price.or_else(|| order.limit_price.take());
  let stop_price = stop_price.or_else(|| order.stop_price.take());

  let quantity = match (quantity, value) {
    (None, None) => order.quantity,
    (Some(quantity), None) => quantity,
    (None, Some(value)) => value_to_quantity(&client, &order.symbol, &value, limit_price.clone())
      .await
      .with_context(|| "unable to convert value to quantity")?,
    _ => unreachable!(),
  };

  let request = order::ChangeReqInit {
    quantity,
    time_in_force,
    limit_price,
    stop_price,
    ..Default::default()
  }
  .init();

  let order = client
    .issue::<order::Patch>(&(id.0, request))
    .await
    .with_context(|| format!("failed to change order {}", id.0.to_hyphenated_ref()))?;

  println!("{}", order.id.to_hyphenated_ref());
  Ok(())
}


/// Cancel an open order.
async fn order_cancel(client: Client, cancel: CancelOrder) -> Result<()> {
  match cancel {
    CancelOrder::ById(id) => client
      .issue::<order::Delete>(&id.0)
      .await
      .with_context(|| "failed to cancel order"),
    CancelOrder::All => {
      // TODO: This isn't quite sufficient if there are more than 500
      //       open orders (unlikely but possible).
      let request = orders::OrdersReq {
        status: orders::Status::Open,
        limit: 500,
        // No need to retrieve nested orders here, they should be
        // canceled automatically when the "parent" is canceled.
        nested: false,
      };
      let orders = client
        .issue::<orders::Get>(&request)
        .await
        .with_context(|| "failed to list orders")?;

      orders
        .into_iter()
        .map(|order| {
          let id = order.id;
          client.issue::<order::Delete>(&id).map_err(move |e| {
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


/// Retrieve information about an order.
async fn order_get(client: Client, id: OrderId) -> Result<()> {
  let currency = client.issue::<account::Get>(&());
  let order = client.issue::<order::Get>(&id.0);

  let (currency, order) = join!(currency, order);
  let currency = currency
    .with_context(|| "failed to retrieve account information")?
    .currency;

  let order = order.with_context(|| "failed to retrieve order information")?;
  let legs = order
    .legs
    .into_iter()
    .map(|order| order.id.to_hyphenated_ref().to_string())
    .collect::<Vec<_>>()
    .join(",");

  println!(
    r#"{sym}:
  order id:         {id}
  status:           {status}
  created at:       {created}
  submitted at:     {submitted}
  updated at:       {updated}
  filled at:        {filled}
  expired at:       {expired}
  canceled at:      {canceled}
  quantity:         {quantity}
  filled quantity:  {filled_qty}
  type:             {type_}
  side:             {side}
  good until:       {good_until}
  limit:            {limit}
  stop:             {stop}
  extended hours:   {extended_hours}
  legs:             {legs}"#,
    sym = order.symbol,
    id = order.id.to_hyphenated_ref(),
    status = format_order_status(order.status),
    created = format_local_time(order.created_at),
    submitted = order
      .submitted_at
      .map(format_local_time)
      .unwrap_or_else(|| "N/A".into()),
    updated = order
      .updated_at
      .map(format_local_time)
      .unwrap_or_else(|| "N/A".into()),
    filled = order
      .filled_at
      .map(format_local_time)
      .unwrap_or_else(|| "N/A".into()),
    expired = order
      .expired_at
      .map(format_local_time)
      .unwrap_or_else(|| "N/A".into()),
    canceled = order
      .canceled_at
      .map(format_local_time)
      .unwrap_or_else(|| "N/A".into()),
    quantity = order.quantity,
    filled_qty = order.filled_quantity,
    type_ = format_order_type(order.type_),
    side = format_order_side(order.side),
    limit = format_option_price(&order.limit_price, &currency),
    stop = format_option_price(&order.stop_price, &currency),
    good_until = format_time_in_force(order.time_in_force),
    extended_hours = order.extended_hours,
    legs = if !legs.is_empty() { legs } else { "N/A".into() },
  );
  Ok(())
}


/// Determine the maximum width of values produced by applying a
/// function on each element of a slice.
fn max_width<T, F>(slice: &[T], f: F) -> usize
where
  F: Fn(&T) -> usize,
{
  slice.iter().fold(0, |m, i| max(m, f(i)))
}

/// Print details of an order.
fn order_print(
  order: &order::Order,
  indent: &str,
  currency: &str,
  side_max: usize,
  qty_max: usize,
  sym_max: usize,
) -> Result<()> {
  let quantity = i32::try_from(order.quantity)
    .with_context(|| format!("order quantity ({}) does not fit into i32", order.quantity))?;
  let time_in_force = format_time_in_force_short(order.time_in_force);

  let summary = match (&order.limit_price, &order.stop_price) {
    (Some(limit), Some(stop)) => {
      debug_assert!(order.type_ == order::Type::StopLimit, "{:?}", order.type_);
      format!(
        "stop @ {}, limit @ {} = {}",
        format_price(stop, currency),
        format_price(limit, currency),
        format_price(&(limit * quantity), currency)
      )
    },
    (Some(limit), None) => {
      debug_assert!(order.type_ == order::Type::Limit, "{:?}", order.type_);
      format!(
        "limit @ {} = {}",
        format_price(limit, currency),
        format_price(&(limit * quantity), currency)
      )
    },
    (None, Some(stop)) => {
      debug_assert!(order.type_ == order::Type::Stop, "{:?}", order.type_);
      format!(
        "stop @ {} = {}",
        format_price(stop, currency),
        format_price(&(stop * quantity), currency)
      )
    },
    (None, None) => {
      debug_assert!(order.type_ == order::Type::Market, "{:?}", order.type_);
      "".to_string()
    },
  };

  println!(
    "[{tif}] {indent}{id} {side:>side_width$} {qty:>qty_width$} {sym:<sym_width$} {summary}",
    tif = time_in_force,
    indent = indent,
    id = order.id.to_hyphenated_ref(),
    side_width = side_max,
    side = format_order_side(order.side),
    qty_width = qty_max,
    qty = format!("{:.0}", order.quantity),
    sym_width = sym_max,
    sym = order.symbol,
    summary = summary,
  );
  Ok(())
}

/// List all currently open orders.
async fn order_list(client: Client, closed: bool) -> Result<()> {
  let request = orders::OrdersReq {
    status: if closed {
      orders::Status::Closed
    } else {
      orders::Status::Open
    },
    limit: 500,
    nested: true,
  };

  let currency = client.issue::<account::Get>(&());
  let orders = client.issue::<orders::Get>(&request);

  let (currency, orders) = join!(currency, orders);
  let currency = currency
    .with_context(|| "failed to retrieve account information")?
    .currency;

  let orders = orders.with_context(|| "failed to list orders")?;

  let side_max = max_width(&orders, |p| format_order_side(p.side).len());
  let qty_max = max_width(&orders, |p| p.quantity.to_string().len());
  let sym_max = max_width(&orders, |p| p.symbol.len());

  for order in orders {
    order_print(&order, "", &currency, side_max, qty_max, sym_max)?;
    for leg in order.legs {
      order_print(&leg, "  ", &currency, side_max, qty_max, sym_max)?;
    }
  }
  Ok(())
}


/// The handler for the 'position' command.
async fn position(client: Client, position: Position) -> Result<()> {
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
async fn position_get(client: Client, symbol: Symbol) -> Result<()> {
  let currency = client.issue::<account::Get>(&());
  let position = client.issue::<position::Get>(&symbol.0);

  let (currency, position) = join!(currency, position);
  let currency = currency
    .with_context(|| "failed to retrieve account information")?
    .currency;
  let position =
    position.with_context(|| format!("failed to retrieve position for {}", symbol.0))?;

  println!(
    r#"{sym}:
  asset id:               {id}
  exchange:               {exchg}
  avg entry:              {entry}
  quantity:               {qty}
  side:                   {side}
  market value:           {value}
  cost basis:             {cost_basis}
  unrealized gain:        {unrealized_gain} ({unrealized_gain_pct})
  unrealized gain today:  {unrealized_gain_today} ({unrealized_gain_today_pct})
  current price:          {current_price}
  last price:             {last_price}"#,
    sym = position.symbol,
    id = position.asset_id.to_hyphenated_ref(),
    exchg = position.exchange.as_ref(),
    entry = format_price(&position.average_entry_price, &currency),
    qty = position.quantity,
    side = format_position_side(position.side),
    value = format_option_price(&position.market_value, &currency),
    cost_basis = format_price(&position.cost_basis, &currency),
    unrealized_gain = format_option_gain(&position.unrealized_gain_total, &currency),
    unrealized_gain_pct = format_option_percent_gain(&position.unrealized_gain_total_percent),
    unrealized_gain_today = format_option_gain(&position.unrealized_gain_today, &currency),
    unrealized_gain_today_pct = format_option_percent_gain(&position.unrealized_gain_today_percent),
    current_price = format_option_price(&position.current_price, &currency),
    last_price = format_option_price(&position.last_day_price, &currency),
  );

  Ok(())
}


/// Liquidate a position for a certain asset.
async fn position_close(client: Client, symbol: Symbol) -> Result<()> {
  let currency = client.issue::<account::Get>(&());
  let order = client.issue::<position::Delete>(&symbol.0);

  let (currency, order) = join!(currency, order);
  let currency = currency
    .with_context(|| "failed to retrieve account information")?
    .currency;
  let order = order.with_context(|| format!("failed to liquidate position for {}", symbol.0))?;

  println!(
    r#"{sym}:
  order id:         {id}
  status:           {status}
  quantity:         {quantity}
  filled quantity:  {filled}
  type:             {type_}
  side:             {side}
  limit:            {limit}
  stop:             {stop}"#,
    sym = order.symbol,
    id = order.id.to_hyphenated_ref(),
    status = format_order_status(order.status),
    quantity = order.quantity,
    filled = order.filled_quantity,
    type_ = format_order_type(order.type_),
    side = format_order_side(order.side),
    limit = format_option_price(&order.limit_price, &currency),
    stop = format_option_price(&order.stop_price, &currency),
  );
  Ok(())
}


/// Format a price value.
fn format_price(price: &Num, currency: &str) -> Str {
  format!("{:.2} {}", price, currency).into()
}

/// Format an optional price value.
fn format_option_price(price: &Option<Num>, currency: &str) -> Str {
  price
    .as_ref()
    .map(|price| format_price(price, currency))
    .unwrap_or_else(|| "N/A".into())
}

fn format_colored<F>(value: &Num, format: F) -> Paint<Str>
where
  F: Fn(&Num) -> Str,
{
  if value.is_positive() {
    Paint::rgb(0x00, 0x70, 0x00, format(value))
  } else if value.is_negative() {
    Paint::red(format(value))
  } else {
    Paint::default(format(value))
  }
}

/// Format gain.
fn format_gain(price: &Num, currency: &str) -> Paint<Str> {
  format_colored(price, |price| format_price(price, currency))
}

/// Format an optional gain.
fn format_option_gain(price: &Option<Num>, currency: &str) -> Paint<Str> {
  price
    .as_ref()
    .map(|price| format_gain(price, currency))
    .unwrap_or_else(|| Paint::default("N/A".into()))
}


/// Format a percentage value.
fn format_percent(percent: &Num) -> Str {
  format!("{:.2}%", percent * 100).into()
}

/// Format an optional percent value.
fn format_option_percent(percent: &Option<Num>) -> Str {
  percent
    .as_ref()
    .map(|percent| format_percent(percent))
    .unwrap_or_else(|| "N/A".into())
}


/// Format percent gain.
fn format_percent_gain(percent: &Num) -> Paint<Str> {
  format_colored(percent, format_percent)
}

/// Format an optional percent gain.
fn format_option_percent_gain(percent: &Option<Num>) -> Paint<Str> {
  percent
    .as_ref()
    .map(|percent| format_percent_gain(percent))
    .unwrap_or_else(|| Paint::default("N/A".into()))
}


/// Format a quantity for a position.
fn format_position_quantity(quantity: u64, side: position::Side) -> String {
  match side {
    position::Side::Long => quantity.into(),
    position::Side::Short => -((quantity as u128) as i128),
  }
  .to_string()
}


/// Print a table with the given positions.
fn position_print(positions: &[position::Position], currency: &str) {
  let qty_max = max_width(positions, |p| {
    format_position_quantity(p.quantity, p.side).len()
  });
  let sym_max = max_width(positions, |p| p.symbol.len());
  let price_max = max_width(positions, |p| {
    format_option_price(&p.current_price, currency).len()
  });
  let value_max = max_width(positions, |p| {
    format_option_price(&p.market_value, currency).len()
  });
  let entry_max = max_width(positions, |p| {
    format_price(&p.average_entry_price, currency).len()
  });
  let today_max = max_width(positions, |p| {
    format_option_price(&p.unrealized_gain_today, currency).len()
  });
  let today_pct_max = max_width(positions, |p| {
    format_option_percent(&p.unrealized_gain_today_percent).len()
  });
  let total_max = max_width(positions, |p| {
    format_option_price(&p.unrealized_gain_total, currency).len()
  });
  let total_pct_max = max_width(positions, |p| {
    format_option_percent(&p.unrealized_gain_total_percent).len()
  });

  // We also need to take the total values into consideration for the
  // maximum width calculation.
  let today_gain = positions.iter().fold(Num::default(), |acc, p| {
    if let Some(gain) = &p.unrealized_gain_today {
      acc + gain
    } else {
      acc
    }
  });
  let base_value = positions.iter().fold(Num::default(), |acc, p| {
    acc
      + if p.side == position::Side::Short {
        -&p.cost_basis
      } else {
        p.cost_basis.clone()
      }
  });
  let total_value = positions.iter().fold(Num::default(), |acc, p| {
    let gain = p
      .unrealized_gain_total
      .as_ref()
      .map(Cow::Borrowed)
      .unwrap_or_else(|| Cow::Owned(Num::from(0)));
    acc
      + if p.side == position::Side::Short {
        -&p.cost_basis + gain.deref()
      } else {
        &p.cost_basis + gain.deref()
      }
  });
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

  let entry_max = max(entry_max, format_price(&base_value, currency).len());
  let today_max = max(today_max, format_price(&today_gain, currency).len());
  let today_pct_max = max(today_pct_max, format_percent(&today_gain_pct).len());
  let total_max = max(total_max, format_price(&total_gain, currency).len());
  let total_pct_max = max(total_pct_max, format_percent(&total_gain_pct).len());

  // TODO: Strictly speaking we should also take into account the
  //       length of the formatted current value.
  let position_col = qty_max + 1 + sym_max + 3 + price_max + 3 + value_max;
  let entry = "Avg Entry";
  let entry_col = max(entry_max, entry.len());
  let today = "Today P/L";
  let today_col = max(today_max + 2 + today_pct_max + 1, today.len());
  let total = "Total P/L";
  let total_col = max(total_max + 2 + total_pct_max + 1, total.len());

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
      "{qty:>qty_width$} {sym:<sym_width$} @ {price:>price_width$} = {value:>value_width$} | \
       {entry:>entry_width$} | \
       {today:>today_width$} ({today_pct:>today_pct_width$}) | \
       {total:>total_width$} ({total_pct:>total_pct_width$})",
      qty_width = qty_max,
      qty = format_position_quantity(position.quantity, position.side),
      sym_width = sym_max,
      sym = position.symbol,
      price_width = price_max,
      price = format_option_price(&position.current_price, currency),
      value_width = value_max,
      value = format_option_price(&position.market_value, currency),
      entry_width = entry_max,
      entry = format_price(&position.average_entry_price, currency),
      today_width = today_max,
      today = format_option_gain(&position.unrealized_gain_today, currency),
      today_pct_width = today_pct_max,
      today_pct = format_option_percent_gain(&position.unrealized_gain_today_percent),
      total_width = total_max,
      total = format_option_gain(&position.unrealized_gain_total, currency),
      total_pct_width = total_pct_max,
      total_pct = format_option_percent_gain(&position.unrealized_gain_total_percent),
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
    "{value:>value_width$}   \
     {base:>base_width$}   \
     {today:>today_width$} ({today_pct:>today_pct_width$})   \
     {total:>total_width$} ({total_pct:>total_pct_width$})",
    value = format_price(&total_value, currency),
    value_width = position_col,
    base = format_price(&base_value, currency),
    base_width = entry_max,
    today = format_gain(&today_gain, currency),
    today_pct = format_percent_gain(&today_gain_pct),
    today_pct_width = today_pct_max,
    today_width = today_max,
    total_width = total_max,
    total = format_gain(&total_gain, currency),
    total_pct_width = total_pct_max,
    total_pct = format_percent_gain(&total_gain_pct),
  );
}

/// List all currently open positions.
async fn position_list(client: Client) -> Result<()> {
  let account = client.issue::<account::Get>(&());
  let positions = client.issue::<positions::Get>(&());

  let (account, positions) = join!(account, positions);
  let account = account.with_context(|| "failed to retrieve account information")?;
  let mut positions = positions.with_context(|| "failed to list positions")?;

  if !positions.is_empty() {
    positions.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    position_print(&positions, &account.currency);
  }
  Ok(())
}

async fn run() -> Result<()> {
  let args = Args::from_args();
  let level = match args.verbosity {
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

  match args.command {
    Command::Account(account) => self::account(client, account).await,
    Command::Asset(asset) => self::asset(client, asset).await,
    Command::Events(events) => self::events(client, events).await,
    Command::Market => self::market(client).await,
    Command::Order(order) => self::order(client, order).await,
    Command::Position(position) => self::position(client, position).await,
  }
}

fn main() {
  let rt = Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap();
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
