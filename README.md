[![pipeline](https://github.com/d-e-s-o/apcacli/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/d-e-s-o/apcacli/actions/workflows/test.yml)
[![crates.io](https://img.shields.io/crates/v/apcacli.svg)](https://crates.io/crates/apcacli)
[![rustc](https://img.shields.io/badge/rustc-1.60+-blue.svg)](https://blog.rust-lang.org/2022/04/07/Rust-1.60.0.html)

apcacli
=======

- [Changelog](CHANGELOG.md)

**apcacli** is a command line application for interacting with the
Alpaca API at [alpaca.markets][]. It provides access to the majority of
Alpaca's functionality, including but not limited to:
- inquiring account information
- changing account configuration
- retrieving account activity
- accessing the market clock
- submitting, changing, listing, and canceling orders
- listing and closing open positions
- listing and retrieving general asset information
- streaming of account and trade events

It supports both the paper trading as well as the live API endpoints.


Usage
-----

The program assumes environment variables representing the Alpaca key ID
(`APCA_API_KEY_ID`) and secret (`APCA_API_SECRET_KEY`), so make sure
that they are present. The program defaults to using the paper trading
API. For live trading you will also need to change the URL to the API
endpoint to use (`APCA_API_BASE_URL`).

```bash
export APCA_API_BASE_URL='https://api.alpaca.markets'; # We trade live
export APCA_API_KEY_ID='XXXXXXXXXXXXXXXXXXXX';
export APCA_API_SECRET_KEY='XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX';
```

With this setup, you can trade from the command line.
##### Submit an Order
```bash
$ apcacli order submit buy SPY --value 1000 --limit-price 200
> 40c13937-5969-48f0-92f8-2f1ef673517a
```

##### Lookup an Order
```bash
$ apcacli order get 40c13937-5969-48f0-92f8-2f1ef673517a
SPY:
  order id:         40c13937-5969-48f0-92f8-2f1ef673517a
  status:           accepted
  created at:       Sun, 10 May 2020 10:15:34 -0700
  submitted at:     Sun, 10 May 2020 10:15:34 -0700
  updated at:       Sun, 10 May 2020 10:15:34 -0700
  filled at:        N/A
  expired at:       N/A
  canceled at:      N/A
  quantity:         5
  filled quantity:  0
  type:             limit
  side:             buy
  good until:       canceled
  limit:            200.00 USD
  stop:             N/A
  extended hours:   false
```

##### List All Open Positions
```
$ apcacli position list
                                   |  Avg Entry  |     Today P/L      |      Total P/L
1 AAPL @  274.02 USD =  274.02 USD |  281.59 USD |  5.65 USD ( 2.11%) |  -7.57 USD (-2.69%)
1 AMZN @ 2359.86 USD = 2359.86 USD | 2426.79 USD | 31.74 USD ( 1.36%) | -66.93 USD (-2.76%)
1 BIDU @  101.75 USD =  101.75 USD |  107.33 USD |  0.34 USD ( 0.34%) |  -5.58 USD (-5.20%)
1 CTSO @    8.90 USD =    8.90 USD |    7.98 USD |  0.50 USD ( 5.95%) |   0.92 USD (11.53%)
1 EA   @  113.00 USD =  113.00 USD |  116.76 USD | -0.27 USD (-0.24%) |  -3.76 USD (-3.22%)
1 SPWR @    6.35 USD =    6.35 USD |    6.66 USD |  0.20 USD ( 3.25%) |  -0.31 USD (-4.65%)
3 XLK  @   87.00 USD =  261.00 USD |   80.10 USD |  8.55 USD ( 3.39%) |  20.70 USD ( 8.61%)
----------------------------------- ------------- -------------------- --------------------
                       3124.88 USD   3187.41 USD   46.71 USD ( 1.47%)   -62.53 USD (-1.96%)
```

More commands are available and can be discovered using the help.

The program is powered by the [`apca`][apca] crate and written in Rust.
It comes with shell completion support and automatic coloring of
profit/losses.


### Shell Completion
As mentioned earlier, **apcacli** comes with shell completion support
(for various shells). A completion script can be generated via the
`shell-complete` utility program and then only needs to be sourced to
make the current shell provide context-sensitive tab completion support.
E.g.,
```bash
$ cargo run --bin=shell-complete > apcacli.bash
$ source apcacli.bash
```

The generated completion script can be installed system-wide as usual
and sourced through initialization files, such as `~/.bashrc`.

Completion scripts for other shells work in a similar manner. Please
refer to the help text (`--help`) of the `shell-complete` program for
the list of supported shells.

[alpaca.markets]: https://alpaca.markets
[apca]: https://crates.io/crates/apca
