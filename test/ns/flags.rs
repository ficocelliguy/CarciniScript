//! `ns.flags` with a typed schema: each default is a string, number, boolean or `vec![]` (or a
//! `FlagValue`), and the result is read by name. The schema is the game's own `[name, default]`
//! pairs (nothing converts).

pub async fn main(ns: NS) {
    let f = ns.flags(&[
        ("target", "n00dles"),
        ("threads", 1),
        ("fast", false),
        ("exclude", vec![]),
        ("delay", FlagValue::Num(2.5)),
    ]);
    let target = f.get_str("target").unwrap_or_default();
    let threads = f.get_f64("threads").unwrap_or(1.0);
    let fast = f.get_bool("fast").unwrap_or(false);
    let exclude = f.get_list("exclude").unwrap_or_default();
    ns.tprint(&format!("{target} x{threads} fast={fast} exclude={exclude:?} rest={:?}", f.rest));
    ns.tprint(&format!("{:?} {:?} {:?}", f.get_f64("delay"), f.get_str("threads"), f.get_bool("missing")));
    match f.get("threads") {
        Some(FlagValue::Num(n)) => ns.tprint(&format!("threads is a number: {n}")),
        other => ns.tprint(&format!("unexpected: {other:?}")),
    }
    ns.tprint(&format!("{f:?}"));
    // A schema built ahead of time is a `Vec` of the same pairs: `.into()` or `FlagValue::from`
    // makes a default from its payload.
    let mut schema: Vec<(&str, FlagValue)> = vec![("fast", FlagValue::Bool(false))];
    schema.push(("target", "home".into()));
    schema.push(("threads", 2.into()));
    schema.push(("exclude", FlagValue::from(vec![])));
    let host = String::from("n00dles");
    schema.push(("from", FlagValue::from(host)));
    let g = ns.flags(&schema);
    ns.tprint(&format!("{:?} {:?}", g.get_bool("fast"), g.get_str("from")));
}
