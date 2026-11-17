// Hooks past `use_state`: a reducer, a memo, a callback, a context with its provider, and a ref
// to an element; handlers of the event, of nothing, and of a `JsValue`; typed attributes
// (`Option`s left out, `bool`s, `style` pairs); `{..spread}` props on an element and from a
// struct's fields on a component; `key` after other attributes; a comment child.
use react::{
    create_context, jsx, use_callback, use_context, use_effect, use_memo, use_reducer, use_ref,
    use_state, Context, Node,
};

#[derive(Clone, Debug)]
enum Action {
    Add(u32),
    Reset,
}

/// The reducer: changes its copy of the log in place and returns it.
fn reduce(mut log: Vec<u32>, action: Action) -> Vec<u32> {
    match action {
        Action::Add(n) => log.push(n),
        Action::Reset => log.clear(),
    }
    log
}

struct BadgeProps {
    label: String,
    count: u32,
}

fn badge(label: String, count: u32) -> Node {
    jsx! { <span class="badge" title={format!("{label}: {count}")}>{label} " " {count}</span> }
}

pub async fn main(ns: NS) {
    let theme: Context<String> = create_context(String::from("light"));
    let themed = {
        let theme = theme.clone();
        move || -> Node {
            let t = use_context(&theme);
            jsx! { <em class={format!("theme-{t}")}>{t}</em> }
        }
    };
    let app = {
        let ns = ns.clone();
        move || -> Node {
            let (log, dispatch) = use_reducer(reduce, Vec::new());
            let total: u32 = {
                let (log, n) = (log.clone(), log.len());
                use_memo(move || log.iter().sum(), (n,))
            };
            let (last, set_last) = use_state(|| String::from("none"));
            let (count, set_count) = use_state(|| 0u32);
            let input = use_ref(None::<js::Element>);
            {
                let input = input.clone();
                let ns = ns.clone();
                use_effect(
                    move || {
                        if let Some(el) = input.get() {
                            ns.print(format!("ref: {}", el.tag_name.to_lowercase()));
                        }
                    },
                    (),
                );
            }
            let add = {
                let d = dispatch.clone();
                use_callback(move |_e: js::Event| d.dispatch(Action::Add(2)), ())
            };
            let reset = dispatch.clone();
            let bump = set_count.clone();
            let extra = vec![("data-kind", "extra"), ("title", "spread")];
            let props = BadgeProps { label: String::from("hits"), count };
            let empty: Option<String> = None;
            jsx! {
                <theme.Provider value={String::from("dark")}>
                    <div id="app" style={vec![("color", "red"), ("font-size", "12px")]}>
                        <button on_click={add}>"add 2"</button>
                        <button class="reset" on_click={move || reset.dispatch(Action::Reset)}>"reset"</button>
                        <button on_click={move |e: js::Event| set_last.set(e.r#type.clone())}>"event"</button>
                        <button on_click={move |_e: JsValue| bump.update(|c| c + 1)}>"bump"</button>
                        <input ref={input.clone()} value="x" read_only={true} disabled={false} />
                        <p title={empty} tab_index={3}>{format!("{:?} = {}", log, total)} {/* a comment */}</p>
                        <p>"last event: " {last}</p>
                        <badge {..props} />
                        <badge count={7} {..BadgeProps { label: String::from("fixed"), count: 0 }} />
                        <span {..extra} key="s">"spread"</span>
                        <themed />
                    </div>
                    <themed />
                </theme.Provider>
            }
        }
    };
    ns.print_raw(jsx! { <><app /><themed /></> });
}
