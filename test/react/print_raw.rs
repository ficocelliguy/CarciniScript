// What `ns.print_raw` / `tprint_raw` take besides `jsx!`: text, numbers (an `i64` too), `Vec`s
// and `Option`s of them; and a generic `T: IntoNode` child.
use react::{jsx, IntoNode, Node};

fn boxed<T: IntoNode>(label: &str, x: T) -> Node {
    jsx! { <i title={label}>{x}</i> }
}

pub async fn main(ns: NS) {
    ns.print_raw("plain text");
    ns.print_raw(42i64);
    ns.print_raw(2.5);
    ns.print_raw(vec!["a", "b"]);
    ns.tprint_raw(Some(jsx! { <b>"bold"</b> }));
    let none: Option<String> = None;
    ns.print_raw(jsx! {
        <p>
            {boxed("char", 'x')}
            {boxed("list", vec![Some(1u8), None, Some(3)])}
            {none}
            {String::from("owned")}
        </p>
    });
}
