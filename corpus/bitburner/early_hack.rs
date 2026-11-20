//! The classic early-game loop: weaken, grow or hack a target depending on its state.
//! Sends a line to the dashboard port after each action.
//!
//! run early_hack.rs <target>

use std::fmt;

const DASHBOARD_PORT: i32 = 1;

#[derive(Debug, Clone, Copy)]
enum Action {
    Weaken,
    Grow,
    Hack,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Action::Weaken => "weaken",
            Action::Grow => "grow",
            Action::Hack => "hack",
        };
        f.pad(s)
    }
}

struct Thresholds {
    money: f64,
    security: f64,
}

impl Thresholds {
    fn for_target(ns: &NS, target: &str) -> Self {
        Thresholds {
            money: ns.get_server_max_money(target) * 0.75,
            security: ns.get_server_min_security_level(target) + 5.0,
        }
    }

    fn choose(&self, ns: &NS, target: &str) -> Action {
        if ns.get_server_security_level(target) > self.security {
            Action::Weaken
        } else if ns.get_server_money_available(target) < self.money {
            Action::Grow
        } else {
            Action::Hack
        }
    }
}

pub async fn main(ns: NS) {
    let target = match ns.args.first().and_then(|a| a.as_string()) {
        Some(t) => t,
        None => ns.get_hostname(),
    };
    let host = ns.get_hostname();
    let limits = Thresholds::for_target(&ns, &target);
    let port = ns.get_port_handle(DASHBOARD_PORT);

    loop {
        let action = limits.choose(&ns, &target);
        let amount = match action {
            Action::Weaken => ns.weaken(&target).await,
            Action::Grow => ns.grow(&target).await,
            Action::Hack => ns.hack(&target).await,
        };
        let line = format!("{host}|{target}|{action}|{amount}");
        // Drop the message rather than block if the dashboard isn't keeping up.
        let _ = port.try_write(line);
    }
}
