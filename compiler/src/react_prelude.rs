// (react prelude)
// What CarciniScript compiles in for the bundled `react` crate and `jsx!`: typed hooks and
// nodes over React (`globalThis.React`, which Bitburner provides), through `κσ.$react`.

/// (A `jsx!` child: a node, text, a number, or an `Option` / `Vec` of them; checked when it
/// renders.)
pub fn __rx_child<T>(t: T) -> ReactNode {
    js::__react_node(t)
}

/// (A component's `children`, written in `jsx!`: static, so React doesn't ask for keys.)
pub fn __rx_children(nodes: Vec<ReactNode>) -> Vec<ReactNode> {
    js::__react_static(nodes)
}

/// `react::h(tag, props, children)`: `React.createElement` for an HTML tag.
pub fn __rx_h(tag: &str, props: Vec<(&str, JsValue)>, children: Vec<ReactNode>) -> ReactNode {
    js::__react_h(tag, props, children)
}

/// (A `jsx!` attribute's value, any JS value.)
pub fn __rx_attr<T>(v: T) -> JsValue {
    js::__react_any(v)
}

/// (An `on*` attribute: a handler that gets the event.)
pub fn __rx_handler<F: Fn(JsValue)>(f: F) -> JsValue {
    js::__react_any(f)
}

/// The setter `use_state` gives: `set(v)`, or `update(|old| new)`.
#[derive(Clone)]
pub struct ReactSetState<T> {
    __f: JsValue,
    __t: Option<T>,
}

impl<T> ReactSetState<T> {
    pub fn set(&self, v: T) {
        js::__react_call(self.__f.clone(), v);
    }
    pub fn update<F: Fn(T) -> T>(&self, f: F) {
        js::__react_call(self.__f.clone(), f);
    }
}

/// `React.useState(init)`: the value now, and its setter.
pub fn __rx_use_state<T, F: Fn() -> T>(init: F) -> (T, ReactSetState<T>) {
    let pair: (T, JsValue) = js::__react_use_state(init);
    (pair.0, ReactSetState { __f: pair.1, __t: None })
}

/// `React.useEffect(f, deps)`: run `f` after a render where a dep changed (`()`: after the first
/// render only; a tuple of values otherwise). `f` may return a cleanup closure.
pub fn __rx_use_effect<R, F: Fn() -> R, D>(f: F, deps: D) {
    js::__react_use_effect(f, deps);
}

/// A box kept across renders (`React.useRef`): `get()` / `set(v)`.
#[derive(Clone)]
pub struct ReactRef<T> {
    __r: JsValue,
    __t: Option<T>,
}

impl<T> ReactRef<T> {
    pub fn get(&self) -> T {
        js::__react_get(self.__r.clone())
    }
    pub fn set(&self, v: T) {
        js::__react_set(self.__r.clone(), v);
    }
}

/// `React.useRef(init)`.
pub fn __rx_use_ref<T>(init: T) -> ReactRef<T> {
    ReactRef { __r: js::__react_use_ref(init), __t: None }
}

/// (`<f a={..} b={..} />` for a component `f`: its arguments, by parameter name.)
pub fn __rx_c0<Comp: Fn() -> ReactNode>(comp: Comp, key: JsValue, names: Vec<&str>) -> ReactNode {
    js::__react_h(js::__react_component(comp, names), vec![("key", key)], Vec::<ReactNode>::new())
}
pub fn __rx_c1<A, Comp: Fn(A) -> ReactNode>(comp: Comp, key: JsValue, names: Vec<&str>, a: A) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c2<A, B, Comp: Fn(A, B) -> ReactNode>(comp: Comp, key: JsValue, names: Vec<&str>, a: A, b: B) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c3<A, B, C, Comp: Fn(A, B, C) -> ReactNode>(comp: Comp, key: JsValue, names: Vec<&str>, a: A, b: B, c: C) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b)), (names[2], __rx_attr(c))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c4<A, B, C, D, Comp: Fn(A, B, C, D) -> ReactNode>(comp: Comp, key: JsValue, names: Vec<&str>, a: A, b: B, c: C, d: D) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b)), (names[2], __rx_attr(c)), (names[3], __rx_attr(d))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c5<A, B, C, D, E, Comp: Fn(A, B, C, D, E) -> ReactNode>(comp: Comp, key: JsValue, names: Vec<&str>, a: A, b: B, c: C, d: D, e: E) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b)), (names[2], __rx_attr(c)), (names[3], __rx_attr(d)), (names[4], __rx_attr(e))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c6<A, B, C, D, E, F, Comp: Fn(A, B, C, D, E, F) -> ReactNode>(comp: Comp, key: JsValue, names: Vec<&str>, a: A, b: B, c: C, d: D, e: E, f: F) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b)), (names[2], __rx_attr(c)), (names[3], __rx_attr(d)), (names[4], __rx_attr(e)), (names[5], __rx_attr(f))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
