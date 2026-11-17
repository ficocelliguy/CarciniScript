// (net prelude)
// `std::net`'s addresses, compiled in when a program names them: `IpAddr`, `Ipv4Addr`,
// `Ipv6Addr`, `SocketAddr`, `SocketAddrV4` / `V6` and `AddrParseError`, parsed and formatted as
// std does. A script has no sockets of its own (Bitburner's servers are reached through `ns`). A
// rewrite (`bundled.rs`) gives these the `Net` names below.

/// `std::net::AddrParseError`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetAddrKind {
    Ip,
    Ipv4,
    Ipv6,
    Socket,
    SocketV4,
    SocketV6,
}

#[derive(Clone, PartialEq, Eq)]
pub struct NetAddrParseError(NetAddrKind);

impl std::fmt::Debug for NetAddrParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "AddrParseError({:?})", self.0)
    }
}

impl std::fmt::Display for NetAddrParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let text = match self.0 {
            NetAddrKind::Ip => "invalid IP address syntax",
            NetAddrKind::Ipv4 => "invalid IPv4 address syntax",
            NetAddrKind::Ipv6 => "invalid IPv6 address syntax",
            NetAddrKind::Socket => "invalid socket address syntax",
            NetAddrKind::SocketV4 => "invalid IPv4 socket address syntax",
            NetAddrKind::SocketV6 => "invalid IPv6 socket address syntax",
        };
        f.write_str(text)
    }
}

impl std::error::Error for NetAddrParseError {}

// ---- the parser (std's rules: no leading zeros in IPv4 parts, `::` once, an IPv4 tail)

struct NetParser {
    b: Vec<u8>,
    at: usize,
}

impl NetParser {
    fn new(s: &str) -> NetParser {
        NetParser {
            b: s.as_bytes().to_vec(),
            at: 0,
        }
    }
    fn done(&self) -> bool {
        self.at == self.b.len()
    }
    fn peek(&self) -> Option<u8> {
        if self.at < self.b.len() {
            Some(self.b[self.at])
        } else {
            None
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn digit(c: u8, radix: u32) -> Option<u32> {
        let d = match c {
            b'0'..=b'9' => (c - b'0') as u32,
            b'a'..=b'f' => (c - b'a') as u32 + 10,
            b'A'..=b'F' => (c - b'A') as u32 + 10,
            _ => return None,
        };
        if d < radix { Some(d) } else { None }
    }
    // A number below `max`, of at most `max_digits` digits (0: any).
    fn number(&mut self, radix: u32, max_digits: usize, zero_prefix: bool, max: u64) -> Option<u64> {
        let start = self.at;
        let leading_zero = self.peek() == Some(b'0');
        let mut v: u64 = 0;
        let mut n = 0;
        while let Some(c) = self.peek() {
            let Some(d) = NetParser::digit(c, radix) else { break };
            self.at += 1;
            v = v * radix as u64 + d as u64;
            n += 1;
            if v > max || (max_digits > 0 && n > max_digits) {
                self.at = start;
                return None;
            }
        }
        if n == 0 || (!zero_prefix && leading_zero && n > 1) {
            self.at = start;
            return None;
        }
        Some(v)
    }
    fn ipv4(&mut self) -> Option<[u8; 4]> {
        let start = self.at;
        let mut out = [0u8; 4];
        for i in 0..4 {
            if i > 0 && !self.eat(b'.') {
                self.at = start;
                return None;
            }
            match self.number(10, 3, false, 255) {
                Some(v) => out[i] = v as u8,
                None => {
                    self.at = start;
                    return None;
                }
            }
        }
        Some(out)
    }
    // Groups separated by `:`, at most `limit`; an IPv4 address may end them. (count, ipv4)
    fn groups(&mut self, out: &mut Vec<u16>, limit: usize) -> (usize, bool) {
        for i in 0..limit {
            if i < limit - 1 {
                let start = self.at;
                if i == 0 || self.eat(b':') {
                    if let Some(v4) = self.ipv4() {
                        out.push(((v4[0] as u16) << 8) | v4[1] as u16);
                        out.push(((v4[2] as u16) << 8) | v4[3] as u16);
                        return (i + 2, true);
                    }
                }
                self.at = start;
            }
            let start = self.at;
            if i > 0 && !self.eat(b':') {
                return (i, false);
            }
            match self.number(16, 4, true, 0xffff) {
                Some(g) => out.push(g as u16),
                None => {
                    self.at = start;
                    return (i, false);
                }
            }
        }
        (limit, false)
    }
    fn ipv6(&mut self) -> Option<[u16; 8]> {
        let start = self.at;
        let mut head = Vec::new();
        let (head_size, head_v4) = self.groups(&mut head, 8);
        let mut out = [0u16; 8];
        if head_size == 8 {
            for i in 0..8 {
                out[i] = head[i];
            }
            return Some(out);
        }
        if head_v4 || !self.eat(b':') || !self.eat(b':') {
            self.at = start;
            return None;
        }
        let mut tail = Vec::new();
        let (tail_size, _) = self.groups(&mut tail, 8 - (head_size + 1));
        for i in 0..head_size {
            out[i] = head[i];
        }
        for i in 0..tail_size {
            out[8 - tail_size + i] = tail[i];
        }
        Some(out)
    }
    fn port(&mut self) -> Option<u16> {
        if !self.eat(b':') {
            return None;
        }
        self.number(10, 0, true, 0xffff).map(|p| p as u16)
    }
    fn socket_v4(&mut self) -> Option<NetSocketAddrV4> {
        let start = self.at;
        let ip = self.ipv4();
        let port = if ip.is_some() { self.port() } else { None };
        match (ip, port) {
            (Some(o), Some(p)) => Some(NetSocketAddrV4::new(NetIpv4Addr { __o: o }, p)),
            _ => {
                self.at = start;
                None
            }
        }
    }
    fn socket_v6(&mut self) -> Option<NetSocketAddrV6> {
        let start = self.at;
        if !self.eat(b'[') {
            return None;
        }
        let Some(s) = self.ipv6() else {
            self.at = start;
            return None;
        };
        let mut scope = 0u32;
        if self.eat(b'%') {
            match self.number(10, 0, true, 0xffff_ffff) {
                Some(v) => scope = v as u32,
                None => {
                    self.at = start;
                    return None;
                }
            }
        }
        if !self.eat(b']') {
            self.at = start;
            return None;
        }
        match self.port() {
            Some(p) => Some(NetSocketAddrV6::new(NetIpv6Addr { __s: s }, p, 0, scope)),
            None => {
                self.at = start;
                None
            }
        }
    }
}

// ---- IPv4

/// `std::net::Ipv4Addr`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetIpv4Addr {
    __o: [u8; 4],
}

impl NetIpv4Addr {
    pub const LOCALHOST: NetIpv4Addr = NetIpv4Addr { __o: [127, 0, 0, 1] };
    pub const UNSPECIFIED: NetIpv4Addr = NetIpv4Addr { __o: [0, 0, 0, 0] };
    pub const BROADCAST: NetIpv4Addr = NetIpv4Addr {
        __o: [255, 255, 255, 255],
    };
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> NetIpv4Addr {
        NetIpv4Addr { __o: [a, b, c, d] }
    }
    pub fn octets(&self) -> [u8; 4] {
        self.__o
    }
    pub fn to_bits(&self) -> u32 {
        ((self.__o[0] as u32) << 24)
            | ((self.__o[1] as u32) << 16)
            | ((self.__o[2] as u32) << 8)
            | self.__o[3] as u32
    }
    pub fn from_bits(bits: u32) -> NetIpv4Addr {
        NetIpv4Addr::new(
            (bits >> 24) as u8,
            (bits >> 16) as u8,
            (bits >> 8) as u8,
            bits as u8,
        )
    }
    pub fn is_unspecified(&self) -> bool {
        self.__o == [0, 0, 0, 0]
    }
    pub fn is_loopback(&self) -> bool {
        self.__o[0] == 127
    }
    pub fn is_private(&self) -> bool {
        self.__o[0] == 10
            || (self.__o[0] == 172 && self.__o[1] >= 16 && self.__o[1] <= 31)
            || (self.__o[0] == 192 && self.__o[1] == 168)
    }
    pub fn is_link_local(&self) -> bool {
        self.__o[0] == 169 && self.__o[1] == 254
    }
    pub fn is_multicast(&self) -> bool {
        self.__o[0] >= 224 && self.__o[0] <= 239
    }
    pub fn is_broadcast(&self) -> bool {
        self.__o == [255, 255, 255, 255]
    }
    pub fn is_documentation(&self) -> bool {
        (self.__o[0] == 192 && self.__o[1] == 0 && self.__o[2] == 2)
            || (self.__o[0] == 198 && self.__o[1] == 51 && self.__o[2] == 100)
            || (self.__o[0] == 203 && self.__o[1] == 0 && self.__o[2] == 113)
    }
    pub fn to_ipv6_mapped(&self) -> NetIpv6Addr {
        let o = self.__o;
        NetIpv6Addr::new(
            0,
            0,
            0,
            0,
            0,
            0xffff,
            ((o[0] as u16) << 8) | o[1] as u16,
            ((o[2] as u16) << 8) | o[3] as u16,
        )
    }
}

impl std::fmt::Display for NetIpv4Addr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let o = self.__o;
        f.pad(&format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3]))
    }
}

impl std::fmt::Debug for NetIpv4Addr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::str::FromStr for NetIpv4Addr {
    type Err = NetAddrParseError;
    fn from_str(s: &str) -> Result<NetIpv4Addr, NetAddrParseError> {
        let mut p = NetParser::new(s);
        match p.ipv4() {
            Some(o) if p.done() => Ok(NetIpv4Addr { __o: o }),
            _ => Err(NetAddrParseError(NetAddrKind::Ipv4)),
        }
    }
}

impl From<[u8; 4]> for NetIpv4Addr {
    fn from(o: [u8; 4]) -> NetIpv4Addr {
        NetIpv4Addr { __o: o }
    }
}

// ---- IPv6

/// `std::net::Ipv6Addr`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetIpv6Addr {
    __s: [u16; 8],
}

impl NetIpv6Addr {
    pub const LOCALHOST: NetIpv6Addr = NetIpv6Addr {
        __s: [0, 0, 0, 0, 0, 0, 0, 1],
    };
    pub const UNSPECIFIED: NetIpv6Addr = NetIpv6Addr {
        __s: [0, 0, 0, 0, 0, 0, 0, 0],
    };
    #[allow(clippy::too_many_arguments)]
    pub fn new(a: u16, b: u16, c: u16, d: u16, e: u16, f: u16, g: u16, h: u16) -> NetIpv6Addr {
        NetIpv6Addr {
            __s: [a, b, c, d, e, f, g, h],
        }
    }
    pub fn segments(&self) -> [u16; 8] {
        self.__s
    }
    pub fn octets(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        for i in 0..8 {
            out[2 * i] = (self.__s[i] >> 8) as u8;
            out[2 * i + 1] = self.__s[i] as u8;
        }
        out
    }
    pub fn is_unspecified(&self) -> bool {
        self.__s == [0, 0, 0, 0, 0, 0, 0, 0]
    }
    pub fn is_loopback(&self) -> bool {
        self.__s == [0, 0, 0, 0, 0, 0, 0, 1]
    }
    pub fn is_multicast(&self) -> bool {
        (self.__s[0] & 0xff00) == 0xff00
    }
    pub fn to_ipv4_mapped(&self) -> Option<NetIpv4Addr> {
        let s = self.__s;
        if s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0xffff {
            Some(NetIpv4Addr::new(
                (s[6] >> 8) as u8,
                s[6] as u8,
                (s[7] >> 8) as u8,
                s[7] as u8,
            ))
        } else {
            None
        }
    }
    pub fn to_ipv4(&self) -> Option<NetIpv4Addr> {
        let s = self.__s;
        if s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && (s[5] == 0 || s[5] == 0xffff) {
            Some(NetIpv4Addr::new(
                (s[6] >> 8) as u8,
                s[6] as u8,
                (s[7] >> 8) as u8,
                s[7] as u8,
            ))
        } else {
            None
        }
    }
}

impl std::fmt::Display for NetIpv6Addr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.__s;
        let text = if let Some(v4) = self.to_ipv4_mapped() {
            format!("::ffff:{}", v4)
        } else {
            // The longest run of zero groups (two or more) is `::`.
            let (mut best_at, mut best_len, mut at, mut len) = (0, 0, 0, 0);
            for i in 0..8 {
                if s[i] == 0 {
                    if len == 0 {
                        at = i;
                    }
                    len += 1;
                    if len > best_len {
                        best_at = at;
                        best_len = len;
                    }
                } else {
                    len = 0;
                }
            }
            let hex = |part: &[u16]| -> String {
                part.iter()
                    .map(|g| format!("{:x}", g))
                    .collect::<Vec<String>>()
                    .join(":")
            };
            if best_len > 1 {
                format!(
                    "{}::{}",
                    hex(&s[..best_at]),
                    hex(&s[best_at + best_len..])
                )
            } else {
                hex(&s[..])
            }
        };
        f.pad(&text)
    }
}

impl std::fmt::Debug for NetIpv6Addr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::str::FromStr for NetIpv6Addr {
    type Err = NetAddrParseError;
    fn from_str(s: &str) -> Result<NetIpv6Addr, NetAddrParseError> {
        let mut p = NetParser::new(s);
        match p.ipv6() {
            Some(g) if p.done() => Ok(NetIpv6Addr { __s: g }),
            _ => Err(NetAddrParseError(NetAddrKind::Ipv6)),
        }
    }
}

impl From<[u16; 8]> for NetIpv6Addr {
    fn from(s: [u16; 8]) -> NetIpv6Addr {
        NetIpv6Addr { __s: s }
    }
}

// ---- IpAddr

/// `std::net::IpAddr`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NetIpAddr {
    V4(NetIpv4Addr),
    V6(NetIpv6Addr),
}

impl NetIpAddr {
    pub fn is_ipv4(&self) -> bool {
        match self {
            NetIpAddr::V4(_) => true,
            NetIpAddr::V6(_) => false,
        }
    }
    pub fn is_ipv6(&self) -> bool {
        !self.is_ipv4()
    }
    pub fn is_loopback(&self) -> bool {
        match self {
            NetIpAddr::V4(a) => a.is_loopback(),
            NetIpAddr::V6(a) => a.is_loopback(),
        }
    }
    pub fn is_unspecified(&self) -> bool {
        match self {
            NetIpAddr::V4(a) => a.is_unspecified(),
            NetIpAddr::V6(a) => a.is_unspecified(),
        }
    }
    pub fn is_multicast(&self) -> bool {
        match self {
            NetIpAddr::V4(a) => a.is_multicast(),
            NetIpAddr::V6(a) => a.is_multicast(),
        }
    }
}

impl std::fmt::Display for NetIpAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NetIpAddr::V4(a) => std::fmt::Display::fmt(a, f),
            NetIpAddr::V6(a) => std::fmt::Display::fmt(a, f),
        }
    }
}

impl std::fmt::Debug for NetIpAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::str::FromStr for NetIpAddr {
    type Err = NetAddrParseError;
    fn from_str(s: &str) -> Result<NetIpAddr, NetAddrParseError> {
        let mut p = NetParser::new(s);
        if let Some(o) = p.ipv4()
            && p.done()
        {
            return Ok(NetIpAddr::V4(NetIpv4Addr { __o: o }));
        }
        let mut p = NetParser::new(s);
        match p.ipv6() {
            Some(g) if p.done() => Ok(NetIpAddr::V6(NetIpv6Addr { __s: g })),
            _ => Err(NetAddrParseError(NetAddrKind::Ip)),
        }
    }
}

impl From<NetIpv4Addr> for NetIpAddr {
    fn from(a: NetIpv4Addr) -> NetIpAddr {
        NetIpAddr::V4(a)
    }
}

impl From<NetIpv6Addr> for NetIpAddr {
    fn from(a: NetIpv6Addr) -> NetIpAddr {
        NetIpAddr::V6(a)
    }
}

// ---- socket addresses

/// `std::net::SocketAddrV4`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetSocketAddrV4 {
    __ip: NetIpv4Addr,
    __port: u16,
}

impl NetSocketAddrV4 {
    pub fn new(ip: NetIpv4Addr, port: u16) -> NetSocketAddrV4 {
        NetSocketAddrV4 {
            __ip: ip,
            __port: port,
        }
    }
    pub fn ip(&self) -> &NetIpv4Addr {
        &self.__ip
    }
    pub fn port(&self) -> u16 {
        self.__port
    }
    pub fn set_ip(&mut self, ip: NetIpv4Addr) {
        self.__ip = ip;
    }
    pub fn set_port(&mut self, port: u16) {
        self.__port = port;
    }
}

impl std::fmt::Display for NetSocketAddrV4 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.pad(&format!("{}:{}", self.__ip, self.__port))
    }
}

impl std::fmt::Debug for NetSocketAddrV4 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::str::FromStr for NetSocketAddrV4 {
    type Err = NetAddrParseError;
    fn from_str(s: &str) -> Result<NetSocketAddrV4, NetAddrParseError> {
        let mut p = NetParser::new(s);
        match p.socket_v4() {
            Some(a) if p.done() => Ok(a),
            _ => Err(NetAddrParseError(NetAddrKind::SocketV4)),
        }
    }
}

/// `std::net::SocketAddrV6`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetSocketAddrV6 {
    __ip: NetIpv6Addr,
    __port: u16,
    __flowinfo: u32,
    __scope_id: u32,
}

impl NetSocketAddrV6 {
    pub fn new(ip: NetIpv6Addr, port: u16, flowinfo: u32, scope_id: u32) -> NetSocketAddrV6 {
        NetSocketAddrV6 {
            __ip: ip,
            __port: port,
            __flowinfo: flowinfo,
            __scope_id: scope_id,
        }
    }
    pub fn ip(&self) -> &NetIpv6Addr {
        &self.__ip
    }
    pub fn port(&self) -> u16 {
        self.__port
    }
    pub fn flowinfo(&self) -> u32 {
        self.__flowinfo
    }
    pub fn scope_id(&self) -> u32 {
        self.__scope_id
    }
    pub fn set_ip(&mut self, ip: NetIpv6Addr) {
        self.__ip = ip;
    }
    pub fn set_port(&mut self, port: u16) {
        self.__port = port;
    }
}

impl std::fmt::Display for NetSocketAddrV6 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let text = if self.__scope_id == 0 {
            format!("[{}]:{}", self.__ip, self.__port)
        } else {
            format!("[{}%{}]:{}", self.__ip, self.__scope_id, self.__port)
        };
        f.pad(&text)
    }
}

impl std::fmt::Debug for NetSocketAddrV6 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::str::FromStr for NetSocketAddrV6 {
    type Err = NetAddrParseError;
    fn from_str(s: &str) -> Result<NetSocketAddrV6, NetAddrParseError> {
        let mut p = NetParser::new(s);
        match p.socket_v6() {
            Some(a) if p.done() => Ok(a),
            _ => Err(NetAddrParseError(NetAddrKind::SocketV6)),
        }
    }
}

/// `std::net::SocketAddr`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NetSocketAddr {
    V4(NetSocketAddrV4),
    V6(NetSocketAddrV6),
}

impl NetSocketAddr {
    pub fn new(ip: NetIpAddr, port: u16) -> NetSocketAddr {
        match ip {
            NetIpAddr::V4(a) => NetSocketAddr::V4(NetSocketAddrV4::new(a, port)),
            NetIpAddr::V6(a) => NetSocketAddr::V6(NetSocketAddrV6::new(a, port, 0, 0)),
        }
    }
    pub fn ip(&self) -> NetIpAddr {
        match self {
            NetSocketAddr::V4(a) => NetIpAddr::V4(a.__ip),
            NetSocketAddr::V6(a) => NetIpAddr::V6(a.__ip),
        }
    }
    pub fn port(&self) -> u16 {
        match self {
            NetSocketAddr::V4(a) => a.__port,
            NetSocketAddr::V6(a) => a.__port,
        }
    }
    pub fn set_port(&mut self, port: u16) {
        match self {
            NetSocketAddr::V4(a) => a.set_port(port),
            NetSocketAddr::V6(a) => a.set_port(port),
        }
    }
    pub fn set_ip(&mut self, ip: NetIpAddr) {
        *self = NetSocketAddr::new(ip, self.port());
    }
    pub fn is_ipv4(&self) -> bool {
        match self {
            NetSocketAddr::V4(_) => true,
            NetSocketAddr::V6(_) => false,
        }
    }
    pub fn is_ipv6(&self) -> bool {
        !self.is_ipv4()
    }
}

impl std::fmt::Display for NetSocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NetSocketAddr::V4(a) => std::fmt::Display::fmt(a, f),
            NetSocketAddr::V6(a) => std::fmt::Display::fmt(a, f),
        }
    }
}

impl std::fmt::Debug for NetSocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::str::FromStr for NetSocketAddr {
    type Err = NetAddrParseError;
    fn from_str(s: &str) -> Result<NetSocketAddr, NetAddrParseError> {
        let mut p = NetParser::new(s);
        if let Some(a) = p.socket_v4()
            && p.done()
        {
            return Ok(NetSocketAddr::V4(a));
        }
        let mut p = NetParser::new(s);
        match p.socket_v6() {
            Some(a) if p.done() => Ok(NetSocketAddr::V6(a)),
            _ => Err(NetAddrParseError(NetAddrKind::Socket)),
        }
    }
}

impl From<NetSocketAddrV4> for NetSocketAddr {
    fn from(a: NetSocketAddrV4) -> NetSocketAddr {
        NetSocketAddr::V4(a)
    }
}

impl From<NetSocketAddrV6> for NetSocketAddr {
    fn from(a: NetSocketAddrV6) -> NetSocketAddr {
        NetSocketAddr::V6(a)
    }
}

impl From<(NetIpAddr, u16)> for NetSocketAddr {
    fn from(pair: (NetIpAddr, u16)) -> NetSocketAddr {
        NetSocketAddr::new(pair.0, pair.1)
    }
}
