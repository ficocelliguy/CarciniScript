// `JsValue` from Rust values (`"x".into()`, `JsValue::from(..)`) and its `Default` (`undefined`),
// also as a derived field and through `unwrap_or_default`.
#[derive(Default, Debug)]
struct Msg {
    payload: JsValue,
    tries: u32,
}

pub async fn main(ns: &NS) {
    let a: JsValue = "x".into();
    let b: JsValue = String::from("y").into();
    let c: JsValue = 2.5.into();
    let d: JsValue = true.into();
    let e = JsValue::from(7);
    ns.tprint(&format!("{:?} {:?} {:?} {:?} {:?}", a.as_string(), b.as_string(), c.as_f64(), d.as_bool(), e.as_f64()));
    let m = Msg::default();
    let none: Option<JsValue> = None;
    ns.tprint(&format!("{} {} {} {}", JsValue::default().is_undefined(), m.payload.is_undefined(), m.tries, none.unwrap_or_default().is_undefined()));
}
