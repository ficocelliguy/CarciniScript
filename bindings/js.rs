// Built-in bindings for JS globals, merged with the NS bindings. A script reaches them through
// the `js` crate: `js::Date::now()`, `js::document()`, `use js::{console, set_timeout};`.
// (`JSON` and `Promise::all` are built into the compiler: they go by Rust types.)
//
// Beyond the NS bindings' format: a function without `self` is called on the type
// (`Date::now()`) or the crate (`js::alert(..)`), its `#[js]` naming the whole JS callee;
// `#[getter]` is the expression itself rather than a call; `#[array]` makes an array-like
// result (a `NodeList`) a `Vec`.

/// A JS `Date`: a moment, as milliseconds since the Unix epoch. `Date::new()` is now,
/// `Date::from_millis(ms)` a given moment, `Date::local(y, m, d, ..)` one in local time.
pub struct Date {}

impl Date {
    /// Now, as milliseconds since the Unix epoch.
    #[js = "globalThis.Date.now"]
    pub fn now() -> f64;
    /// Now.
    #[js = "new globalThis.Date"]
    pub fn new() -> Date;
    /// The moment `ms` milliseconds after the Unix epoch.
    #[js = "new globalThis.Date"]
    pub fn from_millis(ms: f64) -> Date;
    /// A moment in local time; the month counts from 0 (January), as in JS. Fields past their
    /// range carry over.
    #[js = "new globalThis.Date"]
    pub fn local(year: i32, month: i32, #[optional] day: i32, #[optional] hours: i32, #[optional] minutes: i32, #[optional] seconds: i32, #[optional] ms: i32) -> Date;
    /// Milliseconds since the Unix epoch of a UTC moment (`Date.UTC`); months from 0.
    #[js = "globalThis.Date.UTC"]
    pub fn utc(year: i32, month: i32, #[optional] day: i32, #[optional] hours: i32, #[optional] minutes: i32, #[optional] seconds: i32, #[optional] ms: i32) -> f64;
    /// Milliseconds since the Unix epoch of a date string (ISO 8601 and others), NaN if it
    /// doesn't parse.
    #[js = "globalThis.Date.parse"]
    pub fn parse(text: &str) -> f64;
    /// The date and time as JS writes it (what `{}` shows).
    #[js = "toString"]
    pub fn to_string(&self) -> String;
    /// Milliseconds since the Unix epoch (NaN for an invalid date).
    #[js = "getTime"]
    pub fn get_time(&self) -> f64;
    /// The year (local time).
    #[js = "getFullYear"]
    pub fn get_full_year(&self) -> i32;
    /// The month, 0 (January) to 11 (local time).
    #[js = "getMonth"]
    pub fn get_month(&self) -> i32;
    /// The day of the month, 1 to 31 (local time).
    #[js = "getDate"]
    pub fn get_date(&self) -> i32;
    /// The day of the week, 0 (Sunday) to 6 (local time).
    #[js = "getDay"]
    pub fn get_day(&self) -> i32;
    /// The hour, 0 to 23 (local time).
    #[js = "getHours"]
    pub fn get_hours(&self) -> i32;
    /// The minutes, 0 to 59 (local time).
    #[js = "getMinutes"]
    pub fn get_minutes(&self) -> i32;
    /// The seconds, 0 to 59 (local time).
    #[js = "getSeconds"]
    pub fn get_seconds(&self) -> i32;
    /// The milliseconds, 0 to 999 (local time).
    #[js = "getMilliseconds"]
    pub fn get_milliseconds(&self) -> i32;
    /// The year (UTC).
    #[js = "getUTCFullYear"]
    pub fn get_utc_full_year(&self) -> i32;
    /// The month, 0 (January) to 11 (UTC).
    #[js = "getUTCMonth"]
    pub fn get_utc_month(&self) -> i32;
    /// The day of the month, 1 to 31 (UTC).
    #[js = "getUTCDate"]
    pub fn get_utc_date(&self) -> i32;
    /// The day of the week, 0 (Sunday) to 6 (UTC).
    #[js = "getUTCDay"]
    pub fn get_utc_day(&self) -> i32;
    /// The hour, 0 to 23 (UTC).
    #[js = "getUTCHours"]
    pub fn get_utc_hours(&self) -> i32;
    /// The minutes, 0 to 59 (UTC).
    #[js = "getUTCMinutes"]
    pub fn get_utc_minutes(&self) -> i32;
    /// The seconds, 0 to 59 (UTC).
    #[js = "getUTCSeconds"]
    pub fn get_utc_seconds(&self) -> i32;
    /// The milliseconds, 0 to 999 (UTC).
    #[js = "getUTCMilliseconds"]
    pub fn get_utc_milliseconds(&self) -> i32;
    /// Minutes between UTC and local time (UTC minus local).
    #[js = "getTimezoneOffset"]
    pub fn get_timezone_offset(&self) -> i32;
    /// Set the time, as milliseconds since the Unix epoch.
    #[js = "setTime"]
    pub fn set_time(&self, ms: f64) -> f64;
    /// Set the year (local time); month and day optional.
    #[js = "setFullYear"]
    pub fn set_full_year(&self, year: i32, #[optional] month: i32, #[optional] day: i32) -> f64;
    /// Set the month, 0 to 11 (local time).
    #[js = "setMonth"]
    pub fn set_month(&self, month: i32, #[optional] day: i32) -> f64;
    /// Set the day of the month (local time).
    #[js = "setDate"]
    pub fn set_date(&self, day: i32) -> f64;
    /// Set the hour (local time); minutes, seconds and milliseconds optional.
    #[js = "setHours"]
    pub fn set_hours(&self, hours: i32, #[optional] minutes: i32, #[optional] seconds: i32, #[optional] ms: i32) -> f64;
    /// Set the minutes (local time).
    #[js = "setMinutes"]
    pub fn set_minutes(&self, minutes: i32, #[optional] seconds: i32, #[optional] ms: i32) -> f64;
    /// Set the seconds (local time).
    #[js = "setSeconds"]
    pub fn set_seconds(&self, seconds: i32, #[optional] ms: i32) -> f64;
    /// Set the milliseconds (local time).
    #[js = "setMilliseconds"]
    pub fn set_milliseconds(&self, ms: i32) -> f64;
    /// Set the year (UTC).
    #[js = "setUTCFullYear"]
    pub fn set_utc_full_year(&self, year: i32, #[optional] month: i32, #[optional] day: i32) -> f64;
    /// Set the month, 0 to 11 (UTC).
    #[js = "setUTCMonth"]
    pub fn set_utc_month(&self, month: i32, #[optional] day: i32) -> f64;
    /// Set the day of the month (UTC).
    #[js = "setUTCDate"]
    pub fn set_utc_date(&self, day: i32) -> f64;
    /// Set the hour (UTC).
    #[js = "setUTCHours"]
    pub fn set_utc_hours(&self, hours: i32, #[optional] minutes: i32, #[optional] seconds: i32, #[optional] ms: i32) -> f64;
    /// Set the minutes (UTC).
    #[js = "setUTCMinutes"]
    pub fn set_utc_minutes(&self, minutes: i32, #[optional] seconds: i32, #[optional] ms: i32) -> f64;
    /// Set the seconds (UTC).
    #[js = "setUTCSeconds"]
    pub fn set_utc_seconds(&self, seconds: i32, #[optional] ms: i32) -> f64;
    /// `2024-03-01T12:00:00.000Z`
    #[js = "toISOString"]
    pub fn to_iso_string(&self) -> String;
    /// `Fri, 01 Mar 2024 12:00:00 GMT`
    #[js = "toUTCString"]
    pub fn to_utc_string(&self) -> String;
    /// `Fri Mar 01 2024` (local time).
    #[js = "toDateString"]
    pub fn to_date_string(&self) -> String;
    /// `12:00:00 GMT+0000 (Coordinated Universal Time)` (local time).
    #[js = "toTimeString"]
    pub fn to_time_string(&self) -> String;
    /// The date and time in the browser's locale (or `locale`, like `"en-US"`).
    #[js = "toLocaleString"]
    pub fn to_locale_string(&self, #[optional] locale: String) -> String;
    /// The date in the browser's locale (or `locale`).
    #[js = "toLocaleDateString"]
    pub fn to_locale_date_string(&self, #[optional] locale: String) -> String;
    /// The time in the browser's locale (or `locale`).
    #[js = "toLocaleTimeString"]
    pub fn to_locale_time_string(&self, #[optional] locale: String) -> String;
}

/// Why `JSON::parse` failed: the message, as serde_json words it (without a position).
pub struct JsonError {}

impl JsonError {
    /// The message (also what `{}` shows).
    #[js = "message"]
    pub fn message(&self) -> String;
    #[js = "toString"]
    pub fn to_string(&self) -> String;
}

// ---- time, randomness, logging

/// The browser console (the devtools, not the script log): `console::log(&x)`.
pub struct console {}

impl console {
    #[js = "console.log"]
    pub fn log(#[variadic] values: JsValue);
    #[js = "console.info"]
    pub fn info(#[variadic] values: JsValue);
    #[js = "console.warn"]
    pub fn warn(#[variadic] values: JsValue);
    #[js = "console.error"]
    pub fn error(#[variadic] values: JsValue);
    #[js = "console.debug"]
    pub fn debug(#[variadic] values: JsValue);
    /// A table of an array or object.
    #[js = "console.table"]
    pub fn table(value: JsValue);
    #[js = "console.clear"]
    pub fn clear();
    /// Start a timer called `label`; `time_end(label)` logs the time since.
    #[js = "console.time"]
    pub fn time(label: &str);
    #[js = "console.timeEnd"]
    pub fn time_end(label: &str);
    #[js = "console.group"]
    pub fn group(#[variadic] label: JsValue);
    #[js = "console.groupEnd"]
    pub fn group_end();
}

/// `Math.random()`: what Rust's `rand` crate would be for.
pub struct Math {}

impl Math {
    /// A random number in [0, 1).
    #[js = "Math.random"]
    pub fn random() -> f64;
}

/// High-resolution time.
pub struct performance {}

impl performance {
    /// Milliseconds (with fractions) since the page started: for timing code.
    #[js = "globalThis.performance.now"]
    pub fn now() -> f64;
    /// When the page started, as milliseconds since the Unix epoch.
    #[js = "globalThis.performance.timeOrigin"]
    #[getter]
    pub fn time_origin() -> f64;
    /// Record a named moment for `measure`.
    #[js = "globalThis.performance.mark"]
    pub fn mark(name: &str);
    /// Record the time between two marks (or since the start).
    #[js = "globalThis.performance.measure"]
    pub fn measure(name: &str, #[optional] start_mark: &str, #[optional] end_mark: &str);
}

/// Cryptographically strong randomness.
pub struct crypto {}

impl crypto {
    /// A random version 4 UUID (`36b8f84d-df4e-4d49-b662-bcde71a8764f`).
    #[js = "globalThis.crypto.randomUUID"]
    pub fn random_uuid() -> String;
    /// `n` random bytes.
    #[js = "κσ.$randomBytes"]
    pub fn random_bytes(n: usize) -> Vec<u8>;
}

// ---- timers and dialogs

/// Wait `ms` milliseconds (`.await` it). In a Bitburner script, prefer `ns.sleep`.
#[js = "κσ.$sleep"]
pub async fn sleep(ms: f64);
/// Run `handler` after `ms` milliseconds; returns an id for `clear_timeout`.
#[js = "globalThis.setTimeout"]
pub fn set_timeout(handler: impl Fn(), ms: f64) -> f64;
#[js = "globalThis.clearTimeout"]
pub fn clear_timeout(id: f64);
/// Run `handler` every `ms` milliseconds; returns an id for `clear_interval`.
#[js = "globalThis.setInterval"]
pub fn set_interval(handler: impl Fn(), ms: f64) -> f64;
#[js = "globalThis.clearInterval"]
pub fn clear_interval(id: f64);
/// Run `handler` before the next repaint, with the time (`performance::now()`).
#[js = "globalThis.requestAnimationFrame"]
pub fn request_animation_frame(handler: impl Fn(f64)) -> f64;
#[js = "globalThis.cancelAnimationFrame"]
pub fn cancel_animation_frame(id: f64);
/// Run `handler` once the current script step finishes.
#[js = "globalThis.queueMicrotask"]
pub fn queue_microtask(handler: impl Fn());
/// A message box.
#[js = "globalThis.alert"]
pub fn alert(message: &str);
/// An OK / Cancel box: whether OK was chosen.
#[js = "globalThis.confirm"]
pub fn confirm(message: &str) -> bool;
/// A text box: the text, or `None` if cancelled.
#[js = "globalThis.prompt"]
pub fn prompt(message: &str, #[optional] default: &str) -> Option<String>;

// ---- text encoding

/// (The bundled `regex`'s engine: "" if `pattern` compiles, else why not.)
#[js = "κσ.$regex.check"]
pub fn __regex_check(pattern: &str) -> String;
/// (The names of `pattern`'s groups, "" for unnamed ones, the whole match first.)
#[js = "κσ.$regex.names"]
pub fn __regex_names(pattern: &str) -> Vec<String>;
/// (The first match at or after byte `start`: each group's start and end bytes, -1 for a group
/// that didn't take part; empty for no match.)
#[js = "κσ.$regex.find"]
pub fn __regex_find(pattern: &str, hay: &str, start: usize) -> Vec<i32>;
/// (How many bytes the character at byte `at` takes.)
#[js = "κσ.$regex.charLen"]
pub fn __regex_char_len(hay: &str, at: usize) -> usize;

/// A float as serde_json writes it (for the bundled `serde_json`).
#[js = "κσ.$ryu"]
pub fn __json_float(x: f64) -> String;

/// Base64 of a string of Latin-1 characters.
#[js = "globalThis.btoa"]
pub fn btoa(data: &str) -> String;
/// The text a base64 string encodes.
#[js = "globalThis.atob"]
pub fn atob(base64: &str) -> String;
/// `a b&c` → `a%20b%26c`
#[js = "globalThis.encodeURIComponent"]
pub fn encode_uri_component(text: &str) -> String;
#[js = "globalThis.decodeURIComponent"]
pub fn decode_uri_component(text: &str) -> String;

// ---- globals by name

/// A property of `globalThis`, untyped (`None`-like `undefined` when it isn't set): state
/// shared between scripts.
/// A deep copy (`structuredClone`), with the argument's type: JS values (`JsValue`, NS objects)
/// as the browser copies them, Rust values by their `Clone`.
#[js = "κσ.$structuredClone"]
pub fn structured_clone(value: JsValue) -> JsValue;

#[js = "κσ.$global"]
pub fn global(name: &str) -> JsValue;
/// Set a property of `globalThis`.
#[js = "κσ.$setGlobal"]
pub fn set_global(name: &str, value: JsValue);

// ---- storage

/// `local_storage()` (kept across reloads) and `session_storage()` (this tab only).
pub struct Storage {
    /// How many keys are stored.
    #[js = "length"]
    pub length: u32,
}

impl Storage {
    /// The value stored under `key`, if any.
    #[js = "getItem"]
    pub fn get_item(&self, key: &str) -> Option<String>;
    #[js = "setItem"]
    pub fn set_item(&self, key: &str, value: &str);
    #[js = "removeItem"]
    pub fn remove_item(&self, key: &str);
    #[js = "clear"]
    pub fn clear(&self);
    /// The `index`th key.
    #[js = "key"]
    pub fn key(&self, index: u32) -> Option<String>;
}

/// The browser's `localStorage`: strings by key, kept across reloads.
#[js = "globalThis.localStorage"]
#[getter]
pub fn local_storage() -> Storage;
/// The browser's `sessionStorage`: strings by key, for this tab.
#[js = "globalThis.sessionStorage"]
#[getter]
pub fn session_storage() -> Storage;

// ---- the page (`document()` and `window()` cost 25 GB in Bitburner, as in JS)

/// The page's `document`.
#[js = "globalThis.document"]
#[getter]
pub fn document() -> Document;
/// The browser `window`.
#[js = "globalThis.window"]
#[getter]
pub fn window() -> Window;
#[js = "globalThis.location"]
#[getter]
pub fn location() -> Location;
#[js = "globalThis.navigator"]
#[getter]
pub fn navigator() -> Navigator;
#[js = "globalThis.history"]
#[getter]
pub fn history() -> History;
#[js = "globalThis.screen"]
#[getter]
pub fn screen() -> Screen;

pub struct Document {
    #[js = "body"]
    pub body: Element,
    #[js = "head"]
    pub head: Element,
    /// The `<html>` element.
    #[js = "documentElement"]
    pub document_element: Element,
    #[js = "title"]
    pub title: String,
    /// The element with focus.
    #[js = "activeElement"]
    pub active_element: Option<Element>,
}

impl Document {
    /// The first element matching a CSS selector.
    #[js = "querySelector"]
    pub fn query_selector(&self, selector: &str) -> Option<Element>;
    /// Every element matching a CSS selector.
    #[js = "querySelectorAll"]
    #[array]
    pub fn query_selector_all(&self, selector: &str) -> Vec<Element>;
    #[js = "getElementById"]
    pub fn get_element_by_id(&self, id: &str) -> Option<Element>;
    #[js = "getElementsByClassName"]
    #[array]
    pub fn get_elements_by_class_name(&self, names: &str) -> Vec<Element>;
    #[js = "getElementsByTagName"]
    #[array]
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<Element>;
    /// A new element (`"div"`), not yet in the page.
    #[js = "createElement"]
    pub fn create_element(&self, tag: &str) -> Element;
    /// Call `handler` for each `event` (`"keydown"`, `"click"`, ..).
    #[js = "addEventListener"]
    pub fn add_event_listener(&self, event: &str, handler: impl Fn(Event));
}

/// An element of the page.
pub struct Element {
    #[js = "id"]
    pub id: String,
    #[js = "className"]
    pub class_name: String,
    #[js = "tagName"]
    pub tag_name: String,
    #[js = "textContent"]
    pub text_content: String,
    #[js = "innerText"]
    pub inner_text: String,
    #[js = "innerHTML"]
    pub inner_html: String,
    #[js = "outerHTML"]
    pub outer_html: String,
    /// An `<input>`'s value.
    #[js = "value"]
    pub value: String,
    /// A checkbox's state.
    #[js = "checked"]
    pub checked: bool,
    #[js = "disabled"]
    pub disabled: bool,
    #[js = "hidden"]
    pub hidden: bool,
    #[js = "children"]
    #[array]
    pub children: Vec<Element>,
    #[js = "parentElement"]
    pub parent_element: Option<Element>,
    #[js = "firstElementChild"]
    pub first_element_child: Option<Element>,
    #[js = "lastElementChild"]
    pub last_element_child: Option<Element>,
    #[js = "nextElementSibling"]
    pub next_element_sibling: Option<Element>,
    #[js = "previousElementSibling"]
    pub previous_element_sibling: Option<Element>,
    #[js = "classList"]
    pub class_list: ClassList,
    #[js = "style"]
    pub style: CssStyle,
    #[js = "clientWidth"]
    pub client_width: f64,
    #[js = "clientHeight"]
    pub client_height: f64,
    #[js = "offsetWidth"]
    pub offset_width: f64,
    #[js = "offsetHeight"]
    pub offset_height: f64,
    #[js = "scrollTop"]
    pub scroll_top: f64,
    #[js = "scrollHeight"]
    pub scroll_height: f64,
}

impl Element {
    #[js = "querySelector"]
    pub fn query_selector(&self, selector: &str) -> Option<Element>;
    #[js = "querySelectorAll"]
    #[array]
    pub fn query_selector_all(&self, selector: &str) -> Vec<Element>;
    /// The nearest ancestor (or itself) matching a selector.
    #[js = "closest"]
    pub fn closest(&self, selector: &str) -> Option<Element>;
    #[js = "matches"]
    pub fn matches(&self, selector: &str) -> bool;
    #[js = "getAttribute"]
    pub fn get_attribute(&self, name: &str) -> Option<String>;
    #[js = "setAttribute"]
    pub fn set_attribute(&self, name: &str, value: &str);
    #[js = "removeAttribute"]
    pub fn remove_attribute(&self, name: &str);
    #[js = "hasAttribute"]
    pub fn has_attribute(&self, name: &str) -> bool;
    /// Add `child` as the last child (moving it if it's in the page).
    #[js = "appendChild"]
    pub fn append_child(&self, child: &Element);
    #[js = "removeChild"]
    pub fn remove_child(&self, child: &Element);
    /// `"beforebegin"`, `"afterbegin"`, `"beforeend"` or `"afterend"`.
    #[js = "insertAdjacentHTML"]
    pub fn insert_adjacent_html(&self, position: &str, html: &str);
    /// Take it out of the page.
    #[js = "remove"]
    pub fn remove(&self);
    #[js = "click"]
    pub fn click(&self);
    #[js = "focus"]
    pub fn focus(&self);
    #[js = "blur"]
    pub fn blur(&self);
    #[js = "scrollIntoView"]
    pub fn scroll_into_view(&self);
    #[js = "getBoundingClientRect"]
    pub fn get_bounding_client_rect(&self) -> DomRect;
    #[js = "addEventListener"]
    pub fn add_event_listener(&self, event: &str, handler: impl Fn(Event));
}

/// An element's classes.
pub struct ClassList {
    #[js = "length"]
    pub length: u32,
}

impl ClassList {
    #[js = "add"]
    pub fn add(&self, name: &str);
    #[js = "remove"]
    pub fn remove(&self, name: &str);
    #[js = "contains"]
    pub fn contains(&self, name: &str) -> bool;
    /// Add it if it's missing, else remove it; whether it's there now.
    #[js = "toggle"]
    pub fn toggle(&self, name: &str) -> bool;
}

/// An element's inline style (or a computed one).
pub struct CssStyle {
    #[js = "cssText"]
    pub css_text: String,
    #[js = "display"]
    pub display: String,
    #[js = "color"]
    pub color: String,
    #[js = "backgroundColor"]
    pub background_color: String,
    #[js = "width"]
    pub width: String,
    #[js = "height"]
    pub height: String,
    #[js = "visibility"]
    pub visibility: String,
}

impl CssStyle {
    /// A property by its CSS name (`"background-color"`).
    #[js = "getPropertyValue"]
    pub fn get_property_value(&self, name: &str) -> String;
    #[js = "setProperty"]
    pub fn set_property(&self, name: &str, value: &str);
    #[js = "removeProperty"]
    pub fn remove_property(&self, name: &str) -> String;
}

/// An element's box, in pixels from the viewport's top left.
pub struct DomRect {
    #[js = "x"]
    pub x: f64,
    #[js = "y"]
    pub y: f64,
    #[js = "width"]
    pub width: f64,
    #[js = "height"]
    pub height: f64,
    #[js = "top"]
    pub top: f64,
    #[js = "left"]
    pub left: f64,
    #[js = "right"]
    pub right: f64,
    #[js = "bottom"]
    pub bottom: f64,
}

/// What an event handler gets. Fields that don't apply to the event are `undefined`.
pub struct Event {
    /// `"click"`, `"keydown"`, ..
    #[js = "type"]
    pub r#type: String,
    #[js = "target"]
    pub target: Element,
    /// A key event's key (`"Enter"`, `"a"`).
    #[js = "key"]
    pub key: String,
    #[js = "ctrlKey"]
    pub ctrl_key: bool,
    #[js = "shiftKey"]
    pub shift_key: bool,
    #[js = "altKey"]
    pub alt_key: bool,
    /// A mouse event's position in the viewport.
    #[js = "clientX"]
    pub client_x: f64,
    #[js = "clientY"]
    pub client_y: f64,
}

impl Event {
    #[js = "preventDefault"]
    pub fn prevent_default(&self);
    #[js = "stopPropagation"]
    pub fn stop_propagation(&self);
}

/// The browser window.
pub struct Window {
    #[js = "innerWidth"]
    pub inner_width: f64,
    #[js = "innerHeight"]
    pub inner_height: f64,
    #[js = "outerWidth"]
    pub outer_width: f64,
    #[js = "outerHeight"]
    pub outer_height: f64,
    #[js = "devicePixelRatio"]
    pub device_pixel_ratio: f64,
    #[js = "scrollX"]
    pub scroll_x: f64,
    #[js = "scrollY"]
    pub scroll_y: f64,
    #[js = "document"]
    pub document: Document,
    #[js = "location"]
    pub location: Location,
    #[js = "navigator"]
    pub navigator: Navigator,
    #[js = "history"]
    pub history: History,
    #[js = "screen"]
    pub screen: Screen,
    #[js = "localStorage"]
    pub local_storage: Storage,
    #[js = "sessionStorage"]
    pub session_storage: Storage,
}

impl Window {
    #[js = "alert"]
    pub fn alert(&self, message: &str);
    #[js = "confirm"]
    pub fn confirm(&self, message: &str) -> bool;
    #[js = "prompt"]
    pub fn prompt(&self, message: &str, #[optional] default: &str) -> Option<String>;
    /// Open a URL in a new tab (or `target`).
    #[js = "open"]
    pub fn open(&self, url: &str, #[optional] target: &str);
    #[js = "scrollTo"]
    pub fn scroll_to(&self, x: f64, y: f64);
    #[js = "scrollBy"]
    pub fn scroll_by(&self, x: f64, y: f64);
    #[js = "focus"]
    pub fn focus(&self);
    /// The style an element ends up with.
    #[js = "getComputedStyle"]
    pub fn get_computed_style(&self, element: &Element) -> CssStyle;
    /// Whether a media query matches (`"(prefers-color-scheme: dark)"`).
    #[js = "matchMedia"]
    pub fn match_media(&self, query: &str) -> MediaQueryList;
    #[js = "addEventListener"]
    pub fn add_event_listener(&self, event: &str, handler: impl Fn(Event));
    #[js = "requestAnimationFrame"]
    pub fn request_animation_frame(&self, handler: impl Fn(f64)) -> f64;
}

pub struct MediaQueryList {
    #[js = "matches"]
    pub matches: bool,
    #[js = "media"]
    pub media: String,
}

/// The page's address.
pub struct Location {
    #[js = "href"]
    pub href: String,
    #[js = "protocol"]
    pub protocol: String,
    #[js = "host"]
    pub host: String,
    #[js = "hostname"]
    pub hostname: String,
    #[js = "port"]
    pub port: String,
    #[js = "pathname"]
    pub pathname: String,
    #[js = "search"]
    pub search: String,
    #[js = "hash"]
    pub hash: String,
    #[js = "origin"]
    pub origin: String,
}

impl Location {
    #[js = "reload"]
    pub fn reload(&self);
    #[js = "assign"]
    pub fn assign(&self, url: &str);
    #[js = "replace"]
    pub fn replace(&self, url: &str);
}

/// The browser.
pub struct Navigator {
    #[js = "userAgent"]
    pub user_agent: String,
    #[js = "language"]
    pub language: String,
    #[js = "languages"]
    #[array]
    pub languages: Vec<String>,
    /// CPU threads.
    #[js = "hardwareConcurrency"]
    pub hardware_concurrency: u32,
    #[js = "onLine"]
    pub on_line: bool,
    #[js = "clipboard"]
    pub clipboard: Clipboard,
}

pub struct Clipboard {}

impl Clipboard {
    /// Copy text (`.await` it).
    #[js = "writeText"]
    pub async fn write_text(&self, text: &str);
    /// The copied text (`.await` it; the browser may ask the player first).
    #[js = "readText"]
    pub async fn read_text(&self) -> String;
}

pub struct History {
    #[js = "length"]
    pub length: u32,
}

impl History {
    #[js = "back"]
    pub fn back(&self);
    #[js = "forward"]
    pub fn forward(&self);
    #[js = "go"]
    pub fn go(&self, delta: i32);
}

pub struct Screen {
    #[js = "width"]
    pub width: f64,
    #[js = "height"]
    pub height: f64,
    #[js = "availWidth"]
    pub avail_width: f64,
    #[js = "availHeight"]
    pub avail_height: f64,
    #[js = "colorDepth"]
    pub color_depth: f64,
}

// ---- URLs and fetch

/// A parsed URL: `Url::parse("https://a.b/c?d=1")`.
pub struct Url {
    #[js = "href"]
    pub href: String,
    #[js = "protocol"]
    pub protocol: String,
    #[js = "host"]
    pub host: String,
    #[js = "hostname"]
    pub hostname: String,
    #[js = "port"]
    pub port: String,
    #[js = "pathname"]
    pub pathname: String,
    #[js = "search"]
    pub search: String,
    #[js = "hash"]
    pub hash: String,
    #[js = "origin"]
    pub origin: String,
    #[js = "username"]
    pub username: String,
    #[js = "password"]
    pub password: String,
    /// The query, by name.
    #[js = "searchParams"]
    pub search_params: UrlSearchParams,
}

impl Url {
    /// The URL, or `None` if `text` isn't one (relative ones need `base`).
    #[js = "κσ.$urlParse"]
    pub fn parse(text: &str, #[optional] base: &str) -> Option<Url>;
    #[js = "toString"]
    pub fn to_string(&self) -> String;
}

/// A query string's parameters (`a=1&b=2`).
pub struct UrlSearchParams {
    #[js = "size"]
    pub size: u32,
}

impl UrlSearchParams {
    /// From a query string (`"a=1&b=2"`, a leading `?` is fine).
    #[js = "new globalThis.URLSearchParams"]
    pub fn new(query: &str) -> UrlSearchParams;
    #[js = "get"]
    pub fn get(&self, name: &str) -> Option<String>;
    #[js = "getAll"]
    pub fn get_all(&self, name: &str) -> Vec<String>;
    #[js = "has"]
    pub fn has(&self, name: &str) -> bool;
    #[js = "set"]
    pub fn set(&self, name: &str, value: &str);
    #[js = "append"]
    pub fn append(&self, name: &str, value: &str);
    #[js = "delete"]
    pub fn delete(&self, name: &str);
    #[js = "sort"]
    pub fn sort(&self);
    /// `a=1&b=2`
    #[js = "toString"]
    pub fn to_string(&self) -> String;
}

/// `fetch(url).await`: an HTTP response.
pub struct Response {
    /// Whether the status is 200-299.
    #[js = "ok"]
    pub ok: bool,
    #[js = "status"]
    pub status: u16,
    #[js = "statusText"]
    pub status_text: String,
    #[js = "url"]
    pub url: String,
}

impl Response {
    /// The body as text (`.await` it); `JSON::parse::<T>(&text)` reads JSON into a type.
    #[js = "text"]
    pub async fn text(&self) -> String;
    /// The body as an untyped JSON value.
    #[js = "json"]
    pub async fn json(&self) -> JsValue;
}

/// Request a URL (`.await` it). `init` is fetch's options object (`method`, `headers`, `body`).
#[js = "globalThis.fetch"]
pub async fn fetch(url: &str, #[optional] init: JsValue) -> Response;
