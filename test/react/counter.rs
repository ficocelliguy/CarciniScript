// Clicks, state updates, a `jsx!` inside a `{..}` expression, and a list driven by state.
use react::{jsx, use_state, Node};

fn row(n: u32) -> Node {
    jsx! { <li key={n}>"item " {n}</li> }
}

pub async fn main(ns: NS) {
    let counter = move || -> Node {
        let (count, set_count) = use_state(|| 1u32);
        let (log, set_log) = use_state(|| Vec::<String>::new());
        let inc = set_count.clone();
        let reset = set_count.clone();
        let add_log = set_log.clone();
        jsx! {
            <div id="counter">
                <button class="inc" on_click={move |_e| inc.update(|c| c + 1)}>"+1"</button>
                <button
                    class="reset"
                    on_click={move |_e| {
                        reset.set(0);
                        add_log.update(|mut l| {
                            l.push(format!("reset"));
                            l
                        });
                    }}
                >
                    "reset"
                </button>
                <span data-count={count}>{count}</span>
                <ul>{(0..count).map(row).collect::<Vec<_>>()}</ul>
                {(count == 0).then(|| jsx! { <em>"empty"</em> })}
                <ol>{log.iter().map(|l| jsx! { <li key={l.clone()}>{l.clone()}</li> }).collect::<Vec<_>>()}</ol>
            </div>
        }
    };
    ns.print_raw(jsx! { <counter /> });
}
