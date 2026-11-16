// The bundled `log` + `env_logger`: the level macros (also with `target:`), `log!`,
// `log_enabled!`, `Level` / `LevelFilter`, `max_level`, and a logger set up with a level. Log
// lines go to stderr; stdout shows what the program sees.
use log::{debug, error, info, log_enabled, trace, warn, Level, LevelFilter};

fn work(n: u32) -> u32 {
    debug!("working on {}", n);
    if n > 3 {
        warn!(target: "work", "big input: {n}");
    }
    n * 2
}

fn main() {
    // No logger yet: nothing is printed, nothing is enabled.
    info!("before the logger");
    println!("enabled before: {}", log_enabled!(Level::Error));
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();
    println!("info {} debug {}", log_enabled!(Level::Info), log_enabled!(Level::Debug));
    println!("max {}", log::max_level());
    info!("started with {} items", 3);
    let total: u32 = (1..=5).map(work).sum();
    error!("total {}", total);
    trace!("not shown");
    log::log!(Level::Warn, "via log! {}", 1);
    println!("{:?} {} {:?} {}", Level::Warn, Level::Info, LevelFilter::Debug, Level::Error < Level::Trace);
    println!("{}", Level::Debug.to_level_filter());
    println!("total {}", total);
}
