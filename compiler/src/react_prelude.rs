// (react prelude)
// What CarciniScript compiles in for the bundled `react` crate and `jsx!`: typed hooks and
// nodes over React (`globalThis.React`, which Bitburner provides), through `κσ.$react`.

/// What `jsx!` shows as a child and `ns.print_raw` prints: a `Node`, text (`&str`, `String`,
/// `char`), a number, a `JsValue`, or an `Option` (`None` shows nothing) or `Vec` of them.
/// (Built in: the checker knows these types; it isn't implemented for your own.)
pub trait ReactIntoNode {}
/// (An attribute that takes text: `class`, `id`, `href`, ..; or an `Option` of it.)
pub trait ReactText {}
/// (An attribute that takes a number or text: `width`, `tab_index`, `value`, ..)
pub trait ReactTextOrNum {}
/// (An attribute that takes a `bool`: `disabled`, `checked`, ..)
pub trait ReactBool {}
/// (`style`: a CSS string, a `Vec` of `(name, value)` pairs, a `HashMap`, or a `JsValue` object.)
pub trait ReactStyle {}
/// (`{..spread}` on an HTML element: a `Vec` of `(name, value)` pairs, a `HashMap` or a
/// `JsValue` object.)
pub trait ReactProps {}
/// (`key`: text or a number.)
pub trait ReactKey {}

/// (A `jsx!` child.)
pub fn __rx_child<T: ReactIntoNode>(t: T) -> ReactNode {
    js::__react_node(t)
}

/// (A component's `children`, written in `jsx!`: static, so React doesn't ask for keys.)
pub fn __rx_children(nodes: &[ReactNode]) -> Vec<ReactNode> {
    js::__react_static(nodes)
}

/// `react::h(tag, props, children)`: `React.createElement` for an HTML tag.
pub fn __rx_h(tag: &str, props: Vec<(&str, JsValue)>, children: Vec<ReactNode>) -> ReactNode {
    js::__react_h(tag, props, children)
}

/// (An HTML element written in `jsx!`: its props in the order written.)
pub fn __rx_el(tag: &str, props: &[(&str, JsValue)], children: &[ReactNode]) -> ReactNode {
    js::__react_h(tag, props, children)
}

/// (A `jsx!` attribute's value, any JS value.)
pub fn __rx_attr<T>(v: T) -> JsValue {
    js::__react_any(v)
}
pub fn __rx_text<T: ReactText>(v: T) -> JsValue {
    js::__react_any(v)
}
pub fn __rx_num<T: ReactTextOrNum>(v: T) -> JsValue {
    js::__react_any(v)
}
pub fn __rx_bool<T: ReactBool>(v: T) -> JsValue {
    js::__react_any(v)
}
pub fn __rx_style<T: ReactStyle>(v: T) -> JsValue {
    js::__react_any(v)
}
pub fn __rx_spread<T: ReactProps>(v: T) -> JsValue {
    js::__react_any(v)
}
pub fn __rx_key<T: ReactKey>(v: T) -> JsValue {
    js::__react_any(v)
}
/// (`ref={r}`: React sets `r` to the DOM element.)
pub fn __rx_ref<T>(r: &ReactRef<T>) -> JsValue {
    r.__r.clone()
}

/// (An `on*` attribute: a handler that gets the event.)
pub fn __rx_on<F: Fn(Event)>(f: F) -> JsValue {
    js::__react_any(f)
}
/// (An `on*` attribute's closure without parameters.)
pub fn __rx_on0<F: Fn()>(f: F) -> JsValue {
    js::__react_any(f)
}
/// (An `on*` attribute's closure of a `JsValue`, the event as it is.)
pub fn __rx_on_js<F: Fn(JsValue)>(f: F) -> JsValue {
    js::__react_any(f)
}
/// (Earlier name of `__rx_on_js`.)
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
        js::__react_set_state(self.__f.clone(), v);
    }
    pub fn update<F: Fn(T) -> T>(&self, f: F) {
        js::__react_update_state(self.__f.clone(), f);
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

/// `React.useMemo(f, deps)`: `f()`, computed again only after a render where a dep changed.
pub fn __rx_use_memo<T, F: Fn() -> T, D>(f: F, deps: D) -> T {
    js::__react_use_memo(f, deps)
}

/// `React.useCallback(f, deps)`: the same closure across renders until a dep changes.
pub fn __rx_use_callback<F, D>(f: F, deps: D) -> F {
    js::__react_use_callback(f, deps)
}

/// The `dispatch` `use_reducer` gives: `dispatch(action)` runs the reducer on the state.
#[derive(Clone)]
pub struct ReactDispatch<A> {
    __f: JsValue,
    __t: Option<A>,
}

impl<A> ReactDispatch<A> {
    pub fn dispatch(&self, action: A) {
        js::__react_call(self.__f.clone(), action);
    }
}

/// `React.useReducer(reducer, init)`: the state now, and a dispatcher of actions. The reducer
/// gets the state (a copy it may change) and an action, and returns the new state.
pub fn __rx_use_reducer<S, A, R: Fn(S, A) -> S>(reducer: R, init: S) -> (S, ReactDispatch<A>) {
    let pair: (S, JsValue) = js::__react_use_reducer(reducer, init);
    (pair.0, ReactDispatch { __f: pair.1, __t: None })
}

/// A box kept across renders (`React.useRef`): `get()` / `set(v)`. Given to an element as
/// `ref={r.clone()}`, it holds the element (a `js::Element`) once it's mounted.
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

/// A value components below a provider read with `use_context` (`React.createContext`).
/// Provide one in `jsx!` with `<ctx.Provider value={..}>..</ctx.Provider>`, or `ctx.provide(..)`.
#[derive(Clone)]
pub struct ReactContext<T> {
    __c: JsValue,
    __t: Option<T>,
}

impl<T> ReactContext<T> {
    /// `<ctx.Provider value={value}>children</ctx.Provider>`.
    pub fn provide(&self, value: T, children: Vec<ReactNode>) -> ReactNode {
        js::__react_provide(self.__c.clone(), js::__react_any(()), value, children)
    }
}

/// `React.createContext(default)`: made once, outside the components that use it.
pub fn __rx_create_context<T>(default: T) -> ReactContext<T> {
    ReactContext { __c: js::__react_create_context(default), __t: None }
}

/// `React.useContext(ctx)`: the value of the nearest provider above, or the default.
pub fn __rx_use_context<T>(ctx: &ReactContext<T>) -> T {
    js::__react_use_context(ctx.__c.clone())
}

/// (`<ctx.Provider value={..}>` in `jsx!`.)
pub fn __rx_provide<T>(ctx: &ReactContext<T>, key: JsValue, value: T, children: &[ReactNode]) -> ReactNode {
    js::__react_provide(ctx.__c.clone(), key, value, children)
}

/// (`<f a={..} b={..} />` for a component `f`: its arguments, by parameter name.)
pub fn __rx_c0<Comp: Fn() -> ReactNode>(comp: Comp, key: JsValue, names: &[&str]) -> ReactNode {
    js::__react_h(js::__react_component(comp, names), vec![("key", key)], Vec::<ReactNode>::new())
}
pub fn __rx_c1<A, Comp: Fn(A) -> ReactNode>(comp: Comp, key: JsValue, names: &[&str], a: A) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c2<A, B, Comp: Fn(A, B) -> ReactNode>(comp: Comp, key: JsValue, names: &[&str], a: A, b: B) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c3<A, B, C, Comp: Fn(A, B, C) -> ReactNode>(comp: Comp, key: JsValue, names: &[&str], a: A, b: B, c: C) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b)), (names[2], __rx_attr(c))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c4<A, B, C, D, Comp: Fn(A, B, C, D) -> ReactNode>(comp: Comp, key: JsValue, names: &[&str], a: A, b: B, c: C, d: D) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b)), (names[2], __rx_attr(c)), (names[3], __rx_attr(d))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c5<A, B, C, D, E, Comp: Fn(A, B, C, D, E) -> ReactNode>(comp: Comp, key: JsValue, names: &[&str], a: A, b: B, c: C, d: D, e: E) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b)), (names[2], __rx_attr(c)), (names[3], __rx_attr(d)), (names[4], __rx_attr(e))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
pub fn __rx_c6<A, B, C, D, E, F, Comp: Fn(A, B, C, D, E, F) -> ReactNode>(comp: Comp, key: JsValue, names: &[&str], a: A, b: B, c: C, d: D, e: E, f: F) -> ReactNode {
    let props = vec![("key", key), (names[0], __rx_attr(a)), (names[1], __rx_attr(b)), (names[2], __rx_attr(c)), (names[3], __rx_attr(d)), (names[4], __rx_attr(e)), (names[5], __rx_attr(f))];
    js::__react_h(js::__react_component(comp, names), props, Vec::<ReactNode>::new())
}
