// `std::net`'s addresses: `IpAddr` / `Ipv4Addr` / `Ipv6Addr` / `SocketAddr` (and `V4` / `V6`)
// parsed and formatted as std does (leading zeros, `::`, embedded IPv4, scopes, widths), their
// predicates and constants, `AddrParseError`, ordering and `IpAddr::V4(..)` patterns.
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;

fn main() {
    for s in ["127.0.0.1", "10.1.2.3", "256.1.1.1", "01.2.3.4", "1.2.3", "::1", "::", "2001:db8::8a2e:370:7334", "fe80::1:0:0:1", "1:0:0:2:0:0:0:3", "::ffff:192.168.1.9", "::1.2.3.4", "1::2::3", "a:b:c:d:e:f:1:2", "0:0:1:0:0:0:0:0", "hello", ""] {
        match s.parse::<IpAddr>() {
            Ok(ip) => println!("{s:?} -> {ip} {:?} v4={} loop={}", ip, ip.is_ipv4(), ip.is_loopback()),
            Err(e) => println!("{s:?} -> error: {e} / {e:?}"),
        }
    }
    for s in ["127.0.0.1:8080", "[::1]:443", "[fe80::1%3]:22", "1.2.3.4:70000", "1.2.3.4", "[::1]", "localhost:80"] {
        match SocketAddr::from_str(s) {
            Ok(a) => println!("{s} -> {a} {:?} ip={} port={}", a, a.ip(), a.port()),
            Err(e) => println!("{s} -> {e}"),
        }
    }
    let v4 = Ipv4Addr::new(192, 168, 0, 7);
    println!("{v4} {:?} private={} {:?}", v4.octets(), v4.is_private(), v4.to_ipv6_mapped());
    println!("[{:>16}] [{:<12}]", v4, Ipv4Addr::LOCALHOST);
    println!("{}", Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 1, 0, 0, 1));
    println!("{}", Ipv6Addr::LOCALHOST);
    let mut sa = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 80);
    sa.set_port(8000);
    println!("{sa} {}", sa.is_ipv6());
    let s4 = SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 9);
    println!("{} {}", s4, s4.ip());
    println!("{:?}", "1.2.3.4".parse::<Ipv4Addr>() == Ok(Ipv4Addr::new(1, 2, 3, 4)));
    println!("{:?}", "1.2.3.4".parse::<Ipv6Addr>().map_err(|e| e.to_string()));
    let mut ips: Vec<IpAddr> = vec!["::2".parse().unwrap(), "9.9.9.9".parse().unwrap(), "1.1.1.1".parse().unwrap()];
    ips.sort();
    println!("{:?}", ips);
    println!("{}", Ipv4Addr::from_bits(0x7f000001));
    match "10.0.0.1".parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => println!("v4 {}", a.octets()[3]),
        Ok(IpAddr::V6(_)) => println!("v6"),
        Err(_) => println!("bad"),
    }
}
