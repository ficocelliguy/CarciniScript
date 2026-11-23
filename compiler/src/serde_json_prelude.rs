// (serde_json prelude)
// The bundled `serde_json`'s types: `Value` (indexing, `get`, `as_*` / `is_*`, `{}` and `{:#}`
// as JSON text, serde_json's `{:?}`) and `Number`; `serde_json::Map` is a `BTreeMap` (keys in
// order, as without serde_json's `preserve_order`). `to_string`, `from_str`, `to_value`
// and `from_value` are built into the compiler (they go by types, like `js::JSON`), and reach
// `Value` through the `__sj_` functions here.

/// `serde_json::Value`.
#[derive(Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(std::collections::BTreeMap<String, Value>),
}

/// `serde_json::Number`: an integer (`__int`, exact) or a float.
#[derive(Clone, PartialEq)]
pub struct Number {
    __int: bool,
    __i: i128,
    __f: f64,
}

static __SJ_NULL: Value = Value::Null;

impl Number {
    pub fn from_f64(f: f64) -> Option<Number> {
        if f.is_finite() {
            Some(Number {
                __int: false,
                __i: 0,
                __f: f,
            })
        } else {
            None
        }
    }
    pub fn is_i64(&self) -> bool {
        self.__int && self.__i >= i64::MIN as i128 && self.__i <= i64::MAX as i128
    }
    pub fn is_u64(&self) -> bool {
        self.__int && self.__i >= 0 && self.__i <= u64::MAX as i128
    }
    pub fn is_f64(&self) -> bool {
        !self.__int
    }
    pub fn as_i64(&self) -> Option<i64> {
        if self.is_i64() { Some(self.__i as i64) } else { None }
    }
    pub fn as_u64(&self) -> Option<u64> {
        if self.is_u64() { Some(self.__i as u64) } else { None }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if self.__int { Some(self.__i as f64) } else { Some(self.__f) }
    }
}

impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.__int {
            write!(f, "{}", self.__i)
        } else {
            write!(f, "{}", js::__json_float(self.__f))
        }
    }
}

impl std::fmt::Debug for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Number({})", self)
    }
}

impl Default for Value {
    fn default() -> Value {
        Value::Null
    }
}

/// What indexes a `Value`: a key of an object (`v["name"]`) or a position in an array (`v[0]`).
pub trait __SjIndex {
    fn __sj_get<'v>(&self, v: &'v Value) -> Option<&'v Value>;
}

impl __SjIndex for usize {
    fn __sj_get<'v>(&self, v: &'v Value) -> Option<&'v Value> {
        match v {
            Value::Array(a) => a.get(*self),
            _ => None,
        }
    }
}

impl __SjIndex for str {
    fn __sj_get<'v>(&self, v: &'v Value) -> Option<&'v Value> {
        match v {
            Value::Object(m) => m.get(self),
            _ => None,
        }
    }
}

impl std::ops::Index<&str> for Value {
    type Output = Value;
    fn index(&self, key: &str) -> &Value {
        match self {
            Value::Object(m) => match m.get(key) {
                Some(v) => v,
                None => &__SJ_NULL,
            },
            _ => &__SJ_NULL,
        }
    }
}

impl std::ops::Index<usize> for Value {
    type Output = Value;
    fn index(&self, i: usize) -> &Value {
        match self {
            Value::Array(a) => match a.get(i) {
                Some(v) => v,
                None => &__SJ_NULL,
            },
            _ => &__SJ_NULL,
        }
    }
}

/// What serde_json's panics call a value's type.
fn __sj_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// `v["k"] = ..` inserts the key (`Null` becomes an empty object first); `v[i] = ..` needs the
// element.
impl std::ops::IndexMut<&str> for Value {
    fn index_mut(&mut self, key: &str) -> &mut Value {
        if let Value::Null = self {
            *self = Value::Object(std::collections::BTreeMap::new());
        }
        let ty = __sj_type(self);
        match self {
            Value::Object(m) => {
                if !m.contains_key(key) {
                    m.insert(key.to_string(), Value::Null);
                }
                &mut *m.get_mut(key).unwrap()
            }
            _ => panic!("cannot access key {:?} in JSON {}", key, ty),
        }
    }
}

impl std::ops::IndexMut<usize> for Value {
    fn index_mut(&mut self, i: usize) -> &mut Value {
        let ty = __sj_type(self);
        match self {
            Value::Array(a) => {
                let len = a.len();
                if i >= len {
                    panic!("cannot access index {} of JSON array of length {}", i, len);
                }
                &mut a[i]
            }
            _ => panic!("cannot access index {} of JSON {}", i, ty),
        }
    }
}

impl Value {
    pub fn get<I: __SjIndex>(&self, index: I) -> Option<&Value> {
        index.__sj_get(self)
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    pub fn is_boolean(&self) -> bool {
        matches!(self, Value::Bool(_))
    }
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }
    pub fn is_i64(&self) -> bool {
        match self {
            Value::Number(n) => n.is_i64(),
            _ => false,
        }
    }
    pub fn is_u64(&self) -> bool {
        match self {
            Value::Number(n) => n.is_u64(),
            _ => false,
        }
    }
    pub fn is_f64(&self) -> bool {
        match self {
            Value::Number(n) => n.is_f64(),
            _ => false,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(n) => n.as_i64(),
            _ => None,
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Number(n) => n.as_u64(),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => n.as_f64(),
            _ => None,
        }
    }
    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Value::Number(n) => Some(n),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_object(&self) -> Option<&std::collections::BTreeMap<String, Value>> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }
    pub fn as_object_mut(&mut self) -> Option<&mut std::collections::BTreeMap<String, Value>> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Value::Object(m) => m.get_mut(key),
            _ => None,
        }
    }
    /// The value, leaving `Null` in its place.
    pub fn take(&mut self) -> Value {
        std::mem::replace(self, Value::Null)
    }
    /// The value at a JSON Pointer (`"/items/0/name"`).
    pub fn pointer(&self, pointer: &str) -> Option<&Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        if !pointer.starts_with('/') {
            return None;
        }
        let mut target = self;
        for raw in pointer[1..].split('/') {
            let token = raw.replace("~1", "/").replace("~0", "~");
            let next = match target {
                Value::Object(m) => m.get(token.as_str()),
                Value::Array(a) => match token.parse::<usize>() {
                    Ok(i) if !(token.len() > 1 && token.starts_with('0')) => a.get(i),
                    _ => None,
                },
                _ => None,
            };
            match next {
                Some(v) => target = v,
                None => return None,
            }
        }
        Some(target)
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", __sj_write(self, f.alternate(), String::new()))
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let pretty = f.alternate();
        match self {
            Value::Null => write!(f, "Null"),
            Value::Bool(b) => write!(f, "Bool({})", b),
            Value::Number(n) => write!(f, "Number({})", n),
            Value::String(s) => write!(f, "String({:?})", s),
            Value::Array(a) => {
                if pretty {
                    write!(f, "Array {:#?}", a)
                } else {
                    write!(f, "Array {:?}", a)
                }
            }
            Value::Object(m) => {
                if pretty {
                    write!(f, "Object {:#?}", m)
                } else {
                    write!(f, "Object {:?}", m)
                }
            }
        }
    }
}

// `v["n"] == 1`, `"x" == v["s"]`: serde_json's comparisons with numbers, strings and bools.
impl PartialEq<str> for Value {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}

impl PartialEq<&str> for Value {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl PartialEq<String> for Value {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == Some(other.as_str())
    }
}

impl PartialEq<Value> for str {
    fn eq(&self, other: &Value) -> bool {
        other.as_str() == Some(self)
    }
}

impl PartialEq<Value> for &str {
    fn eq(&self, other: &Value) -> bool {
        other.as_str() == Some(*self)
    }
}

impl PartialEq<Value> for String {
    fn eq(&self, other: &Value) -> bool {
        other.as_str() == Some(self.as_str())
    }
}

impl PartialEq<bool> for Value {
    fn eq(&self, other: &bool) -> bool {
        self.as_bool() == Some(*other)
    }
}

impl PartialEq<Value> for bool {
    fn eq(&self, other: &Value) -> bool {
        other.as_bool() == Some(*self)
    }
}

impl PartialEq<i8> for Value {
    fn eq(&self, other: &i8) -> bool {
        self.as_i64().map_or(false, |i| i == *other as i64)
    }
}

impl PartialEq<Value> for i8 {
    fn eq(&self, other: &Value) -> bool {
        other.as_i64().map_or(false, |i| i == *self as i64)
    }
}

impl PartialEq<i16> for Value {
    fn eq(&self, other: &i16) -> bool {
        self.as_i64().map_or(false, |i| i == *other as i64)
    }
}

impl PartialEq<Value> for i16 {
    fn eq(&self, other: &Value) -> bool {
        other.as_i64().map_or(false, |i| i == *self as i64)
    }
}

impl PartialEq<i32> for Value {
    fn eq(&self, other: &i32) -> bool {
        self.as_i64().map_or(false, |i| i == *other as i64)
    }
}

impl PartialEq<Value> for i32 {
    fn eq(&self, other: &Value) -> bool {
        other.as_i64().map_or(false, |i| i == *self as i64)
    }
}

impl PartialEq<i64> for Value {
    fn eq(&self, other: &i64) -> bool {
        self.as_i64().map_or(false, |i| i == *other as i64)
    }
}

impl PartialEq<Value> for i64 {
    fn eq(&self, other: &Value) -> bool {
        other.as_i64().map_or(false, |i| i == *self as i64)
    }
}

impl PartialEq<isize> for Value {
    fn eq(&self, other: &isize) -> bool {
        self.as_i64().map_or(false, |i| i == *other as i64)
    }
}

impl PartialEq<Value> for isize {
    fn eq(&self, other: &Value) -> bool {
        other.as_i64().map_or(false, |i| i == *self as i64)
    }
}

impl PartialEq<u8> for Value {
    fn eq(&self, other: &u8) -> bool {
        self.as_u64().map_or(false, |i| i == *other as u64)
    }
}

impl PartialEq<Value> for u8 {
    fn eq(&self, other: &Value) -> bool {
        other.as_u64().map_or(false, |i| i == *self as u64)
    }
}

impl PartialEq<u16> for Value {
    fn eq(&self, other: &u16) -> bool {
        self.as_u64().map_or(false, |i| i == *other as u64)
    }
}

impl PartialEq<Value> for u16 {
    fn eq(&self, other: &Value) -> bool {
        other.as_u64().map_or(false, |i| i == *self as u64)
    }
}

impl PartialEq<u32> for Value {
    fn eq(&self, other: &u32) -> bool {
        self.as_u64().map_or(false, |i| i == *other as u64)
    }
}

impl PartialEq<Value> for u32 {
    fn eq(&self, other: &Value) -> bool {
        other.as_u64().map_or(false, |i| i == *self as u64)
    }
}

impl PartialEq<u64> for Value {
    fn eq(&self, other: &u64) -> bool {
        self.as_u64().map_or(false, |i| i == *other as u64)
    }
}

impl PartialEq<Value> for u64 {
    fn eq(&self, other: &Value) -> bool {
        other.as_u64().map_or(false, |i| i == *self as u64)
    }
}

impl PartialEq<usize> for Value {
    fn eq(&self, other: &usize) -> bool {
        self.as_u64().map_or(false, |i| i == *other as u64)
    }
}

impl PartialEq<Value> for usize {
    fn eq(&self, other: &Value) -> bool {
        other.as_u64().map_or(false, |i| i == *self as u64)
    }
}

impl PartialEq<f32> for Value {
    fn eq(&self, other: &f32) -> bool {
        self.as_f64().map_or(false, |i| i == *other as f64)
    }
}

impl PartialEq<Value> for f32 {
    fn eq(&self, other: &Value) -> bool {
        other.as_f64().map_or(false, |i| i == *self as f64)
    }
}

impl PartialEq<f64> for Value {
    fn eq(&self, other: &f64) -> bool {
        self.as_f64().map_or(false, |i| i == *other as f64)
    }
}

impl PartialEq<Value> for f64 {
    fn eq(&self, other: &Value) -> bool {
        other.as_f64().map_or(false, |i| i == *self as f64)
    }
}

impl std::str::FromStr for Value {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> Result<Value, serde_json::Error> {
        serde_json::from_str(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Value {
        Value::Number(__sj_int(n as i128))
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Value {
        Value::Number(__sj_int(n as i128))
    }
}

impl From<u32> for Value {
    fn from(n: u32) -> Value {
        Value::Number(__sj_int(n as i128))
    }
}

impl From<u64> for Value {
    fn from(n: u64) -> Value {
        Value::Number(__sj_int(n as i128))
    }
}

impl From<usize> for Value {
    fn from(n: usize) -> Value {
        Value::Number(__sj_int(n as i128))
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Value {
        match Number::from_f64(f) {
            Some(n) => Value::Number(n),
            None => Value::Null,
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::String(s)
    }
}

impl From<Vec<Value>> for Value {
    fn from(a: Vec<Value>) -> Value {
        Value::Array(a)
    }
}

impl From<std::collections::BTreeMap<String, Value>> for Value {
    fn from(m: std::collections::BTreeMap<String, Value>) -> Value {
        Value::Object(m)
    }
}

impl From<Number> for Value {
    fn from(n: Number) -> Value {
        Value::Number(n)
    }
}

impl From<()> for Value {
    fn from(_u: ()) -> Value {
        Value::Null
    }
}

fn __sj_int(i: i128) -> Number {
    Number {
        __int: true,
        __i: i,
        __f: 0.0,
    }
}

// What the runtime calls to make a `Value` of parsed JSON.
fn __sj_null() -> Value {
    Value::Null
}

fn __sj_bool(b: bool) -> Value {
    Value::Bool(b)
}

/// A number as serde_json reads it: an integer that fits `u64` or `i64`, else a float.
fn __sj_num(raw: String, int: bool) -> Value {
    if int {
        let i: i128 = raw.parse().unwrap_or(i128::MAX);
        if i >= i64::MIN as i128 && i <= u64::MAX as i128 {
            return Value::Number(__sj_int(i));
        }
    }
    let f: f64 = raw.parse().unwrap_or(0.0);
    match Number::from_f64(f) {
        Some(n) => Value::Number(n),
        None => Value::Null,
    }
}

fn __sj_str(s: String) -> Value {
    Value::String(s)
}

fn __sj_arr(items: Vec<Value>) -> Value {
    Value::Array(items)
}

fn __sj_obj(keys: Vec<String>, values: Vec<Value>) -> Value {
    let mut m = std::collections::BTreeMap::new();
    for i in 0..keys.len() {
        m.insert(keys[i].clone(), values[i].clone());
    }
    Value::Object(m)
}

/// A string as serde_json writes it.
fn __sj_quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `v` as JSON text, compact or pretty (two spaces, nested past `indent`).
fn __sj_write(v: &Value, pretty: bool, indent: String) -> String {
    let mut out = String::new();
    __sj_write_into(v, pretty, &indent, &mut out);
    out
}

fn __sj_write_into(v: &Value, pretty: bool, indent: &str, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => __sj_quote(s, out),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            let inner = format!("{}  ", indent);
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    out.push_str(&inner);
                }
                __sj_write_into(x, pretty, &inner, out);
            }
            if pretty {
                out.push('\n');
                out.push_str(indent);
            }
            out.push(']');
        }
        Value::Object(m) => {
            if m.is_empty() {
                out.push_str("{}");
                return;
            }
            let inner = format!("{}  ", indent);
            out.push('{');
            let mut first = true;
            for (k, x) in m.iter() {
                if !first {
                    out.push(',');
                }
                first = false;
                if pretty {
                    out.push('\n');
                    out.push_str(&inner);
                }
                __sj_quote(k, out);
                out.push(':');
                if pretty {
                    out.push(' ');
                }
                __sj_write_into(x, pretty, &inner, out);
            }
            if pretty {
                out.push('\n');
                out.push_str(indent);
            }
            out.push('}');
        }
    }
}
