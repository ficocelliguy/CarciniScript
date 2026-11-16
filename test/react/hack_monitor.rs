// A tail-window dashboard drawn with `jsx!` (the web REPL's React example): a component with
// props, state, an interval effect with cleanup, and buttons. The `.jsx` twin is what a
// Bitburner `.jsx` script would be.
use react::{jsx, use_effect, use_state, Node};

fn Bar(label: &str, value: f64) -> Node {
    let pct = (value * 100.0).round() as u32;
    jsx! {
        <div class="bar">
            <span>{label} ": " {pct} "%"</span>
            <div style="background: #222; height: 6px">
                <div style={format!("background: #0c0; height: 6px; width: {}%", pct)}></div>
            </div>
        </div>
    }
}

pub async fn main(ns: NS) {
    ns.disable_log("ALL");
    ns.ui.open_tail();
    let target = "n00dles".to_string();
    let monitor = move || -> Node {
        let (money, set_money) = use_state(|| 0.0f64);
        let (running, set_running) = use_state(|| true);
        let (hacks, set_hacks) = use_state(|| 0u32);
        // While running, a "hack" every 20 ms: the interval is cleared when paused.
        let (tick_money, tick_hacks) = (set_money.clone(), set_hacks.clone());
        use_effect(
            move || {
                let id = if running {
                    Some(js::set_interval(
                        move || {
                            tick_money.update(|m| (m + 0.125).min(1.0));
                            tick_hacks.update(|h| h + 1);
                        },
                        20.0,
                    ))
                } else {
                    None
                };
                move || {
                    if let Some(id) = id {
                        js::clear_interval(id);
                    }
                }
            },
            (running,),
        );
        let (toggle, reset_money, reset_hacks) = (set_running.clone(), set_money.clone(), set_hacks.clone());
        let label = if running { "pause" } else { "resume" };
        jsx! {
            <div style="font-family: monospace; padding: 4px">
                <h4>"hacking " {target.clone()}</h4>
                <Bar label="money" value={money} />
                <p>"hacks: " {hacks}</p>
                <button on_click={move |_e| toggle.update(|r| !r)}>{label}</button>
                <button
                    on_click={move |_e| {
                        reset_money.set(0.0);
                        reset_hacks.set(0);
                    }}
                >
                    "reset"
                </button>
                {(money >= 1.0).then(|| jsx! { <strong>"server drained"</strong> })}
            </div>
        }
    };
    ns.print_raw(jsx! { <monitor /> });
}
