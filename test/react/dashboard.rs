use react::{jsx, use_effect, use_state, Node};

/// One labelled bar.
fn gauge(label: &str, value: f64) -> Node {
    let pct = (value * 100.0).round();
    jsx! {
        <div class="gauge" style={format!("width: {pct}%; background-color: #0c0")}>
            {label} ": " {pct} "%"
        </div>
    }
}

fn panel(title: String, children: Vec<Node>) -> Node {
    jsx! {
        <section aria-label={title.clone()}>
            <h3>{title}</h3>
            {children}
        </section>
    }
}

pub async fn main(ns: NS) {
    let servers = vec![("n00dles", 0.25), ("joesguns", 0.8), ("phantasy", 1.0)];
    let ticker = move || -> Node {
        let (ticks, set_ticks) = use_state(|| 0u32);
        use_effect(
            move || {
                let set_ticks = set_ticks.clone();
                let id = js::set_interval(move || set_ticks.update(|t| t + 1), 10.0);
                move || js::clear_interval(id)
            },
            (),
        );
        let note: Option<&str> = (ticks >= 3).then_some("warmed up");
        jsx! { <p>"ticks: " {ticks} " " {note}</p> }
    };
    let rows: Vec<Node> = servers
        .iter()
        .map(|&(host, frac)| jsx! { <li key={host}><gauge label={host} value={frac} /></li> })
        .collect();
    ns.print_raw(jsx! {
        <panel title={String::from("Targets")}>
            <ul>{rows}</ul>
            <ticker />
            <>
                <button on_click={move |_e| {}} disabled>"stop"</button>
                <br />
            </>
        </panel>
    });
}
