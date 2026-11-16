//! Stock trader. With 4S data it trades on the forecast; without it, it estimates the
//! forecast from the last `WINDOW` ticks of price movement.
//!
//! run stocks.rs [--liquidate]

use std::collections::{HashMap, VecDeque};
use std::fmt;

const WINDOW: usize = 20;
const COMMISSION: f64 = 100_000.0;
const BUY_ABOVE: f64 = 0.58;
const SELL_BELOW: f64 = 0.52;
const CASH_RESERVE: f64 = 10e6;

/// A ring buffer of recent prices.
#[derive(Debug, Default)]
struct History<const N: usize> {
    prices: VecDeque<f64>,
}

impl<const N: usize> History<N> {
    fn push(&mut self, price: f64) {
        if self.prices.len() == N {
            self.prices.pop_front();
        }
        self.prices.push_back(price);
    }

    fn is_full(&self) -> bool {
        self.prices.len() == N
    }

    /// Fraction of ticks that went up: a cheap forecast estimate.
    fn up_ratio(&self) -> Option<f64> {
        let (ups, total) = self
            .prices
            .iter()
            .zip(self.prices.iter().skip(1))
            .fold((0, 0), |(u, t), (a, b)| (u + (b > a) as u32, t + 1));
        (total > 0).then(|| f64::from(ups) / f64::from(total))
    }

    fn volatility(&self) -> f64 {
        let returns: Vec<f64> = self
            .prices
            .iter()
            .zip(self.prices.iter().skip(1))
            .map(|(a, b)| (b / a).ln())
            .collect();
        std_dev(&returns)
    }
}

fn mean<I>(xs: I) -> Option<f64>
where
    I: IntoIterator,
    I::Item: Into<f64>,
{
    let (sum, n) = xs.into_iter().fold((0.0, 0usize), |(s, n), x| (s + x.into(), n + 1));
    (n > 0).then(|| sum / n as f64)
}

fn std_dev(xs: &[f64]) -> f64 {
    let Some(m) = mean(xs.iter().copied()) else { return 0.0 };
    mean(xs.iter().map(|x| (x - m).powi(2))).unwrap_or(0.0).sqrt()
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Position {
    long: f64,
    long_avg: f64,
    short: f64,
    short_avg: f64,
}

impl Position {
    fn read(ns: &NS, sym: &str) -> Self {
        let [long, long_avg, short, short_avg]: [f64; 4] = ns.stock.get_position(sym);
        Position { long, long_avg, short, short_avg }
    }

    fn value(&self, price: f64) -> f64 {
        self.long * price + self.short * (2.0 * self.short_avg - price)
    }

    fn is_empty(&self) -> bool {
        self.long == 0.0 && self.short == 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Signal {
    StrongSell,
    Sell,
    Hold,
    Buy,
    StrongBuy,
}

impl Signal {
    fn from_forecast(f: f64) -> Signal {
        match f {
            f if f >= BUY_ABOVE + 0.1 => Signal::StrongBuy,
            f if f >= BUY_ABOVE => Signal::Buy,
            f if f <= 1.0 - BUY_ABOVE - 0.1 => Signal::StrongSell,
            f if f <= 1.0 - BUY_ABOVE => Signal::Sell,
            _ => Signal::Hold,
        }
    }
}

struct Market<'ns> {
    ns: &'ns NS,
    has_4s: bool,
    can_short: bool,
    history: HashMap<String, History<WINDOW>>,
    realized: f64,
}

impl<'ns> Market<'ns> {
    fn forecast(&self, sym: &str) -> Option<f64> {
        if self.has_4s {
            return Some(self.ns.stock.get_forecast(sym));
        }
        let h = self.history.get(sym)?;
        h.is_full().then(|| h.up_ratio()).flatten()
    }

    fn record_prices(&mut self) {
        for sym in self.ns.stock.get_symbols() {
            let price = self.ns.stock.get_price(&sym);
            self.history.entry(sym).or_default().push(price);
        }
    }

    fn sell_all(&mut self, sym: &str, pos: Position) {
        if pos.long > 0.0 {
            let price = self.ns.stock.sell_stock(sym, pos.long);
            self.realized += (price - pos.long_avg) * pos.long - COMMISSION;
        }
        if pos.short > 0.0 && self.can_short {
            let price = self.ns.stock.sell_short(sym, pos.short);
            self.realized += (pos.short_avg - price) * pos.short - COMMISSION;
        }
    }

    fn tick(&mut self) {
        self.record_prices();
        let mut ranked: Vec<(String, f64)> = self
            .ns
            .stock
            .get_symbols()
            .into_iter()
            .filter_map(|s| self.forecast(&s).map(|f| (s, f)))
            .collect();
        // most confident first, either direction
        ranked.sort_by(|a, b| (b.1 - 0.5).abs().total_cmp(&(a.1 - 0.5).abs()));

        for (sym, forecast) in &ranked {
            let pos = Position::read(self.ns, sym);
            let close_long = pos.long > 0.0 && *forecast < SELL_BELOW;
            let close_short = pos.short > 0.0 && *forecast > 1.0 - SELL_BELOW;
            if close_long || close_short {
                self.sell_all(sym, pos);
            }
        }

        for (sym, forecast) in ranked {
            let cash = self.ns.get_server_money_available("home") - CASH_RESERVE;
            if cash < COMMISSION * 20.0 {
                break;
            }
            let pos = Position::read(self.ns, &sym);
            let room = self.ns.stock.get_max_shares(&sym) - pos.long - pos.short;
            let signal = Signal::from_forecast(forecast);
            let ask = self.ns.stock.get_ask_price(&sym);
            let shares = ((cash - COMMISSION) / ask).floor().min(room);
            if shares <= 0.0 {
                continue;
            }
            match signal {
                Signal::Buy | Signal::StrongBuy if pos.short == 0.0 => {
                    self.ns.stock.buy_stock(&sym, shares);
                }
                Signal::Sell | Signal::StrongSell if self.can_short && pos.long == 0.0 => {
                    self.ns.stock.buy_short(&sym, shares);
                }
                _ => {}
            }
        }
    }

    fn portfolio_value(&self) -> f64 {
        self.ns
            .stock
            .get_symbols()
            .iter()
            .map(|s| Position::read(self.ns, s).value(self.ns.stock.get_bid_price(s)))
            .sum()
    }
}

impl fmt::Display for Market<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{:<6} {:>8} {:>7} {:>12}", "SYM", "FCST", "VOL", "VALUE")?;
        let mut rows: Vec<_> = self
            .ns
            .stock
            .get_symbols()
            .into_iter()
            .map(|s| (Position::read(self.ns, &s), s))
            .filter(|(p, _)| !p.is_empty())
            .collect();
        rows.sort_by(|a, b| a.1.cmp(&b.1));
        for (pos, sym) in rows {
            let fc = self.forecast(&sym).map_or("-".to_string(), |f| format!("{:.3}", f));
            let vol = self.history.get(&sym).map_or(0.0, History::volatility);
            writeln!(f, "{sym:<6} {fc:>8} {vol:>7.4} {:>12.0}", pos.value(self.ns.stock.get_price(&sym)))?;
        }
        write!(f, "realized: {:.0}", self.realized)
    }
}

pub async fn main(ns: NS) {
    ns.disable_log("ALL");
    if !ns.stock.has_wse_account() || !ns.stock.has_tix_api_access() {
        ns.tprint("ERROR need a WSE account and TIX API access");
        return;
    }
    let mut market = Market {
        ns: &ns,
        has_4s: ns.stock.has4_s_data_tix_api(),
        can_short: {
            let reset = ns.get_reset_info();
            reset.current_node == 8.0 || reset.owned_sf.get(&8).is_some_and(|&lvl| lvl >= 2.0)
        },
        history: HashMap::new(),
        realized: 0.0,
    };

    if ns.args.iter().any(|a| a.as_string().as_deref() == Some("--liquidate")) {
        for sym in ns.stock.get_symbols() {
            let pos = Position::read(&ns, &sym);
            market.sell_all(&sym, pos);
        }
        ns.tprint(&format!("liquidated, realized {:.0}", market.realized));
        return;
    }

    ns.ui.open_tail();
    loop {
        ns.stock.next_update().await;
        market.tick();
        ns.clear_log();
        ns.print(&market.to_string());
        ns.print(&format!("portfolio: {}", ns.format.number(market.portfolio_value())));
    }
}
