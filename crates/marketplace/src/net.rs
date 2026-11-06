//! What kind of network an address is on (decision 17, ADR-015 amendment).
//!
//! Lullshift lets a marketplace live on the user's own computer or network only after the user
//! confirms it. This module says which addresses need that confirmation and which can never be
//! a Git server at all.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use crate::error::UrlError;
use crate::validate::{GitUrl, Scheme};

/// Where a fetch may go, checked just before it starts (decisions 17 and 18.8).
///
/// - A private host in the address needs the user's confirmation.
/// - The host name is resolved with the system resolver; if any address it resolves to can
///   never be a Git server the fetch is refused, and if any is private the fetch needs the same
///   confirmation as a private address. IPv4 inside IPv6 is judged as IPv4.
/// - If the name doesn't resolve here (no network, or only a proxy can resolve it), nothing is
///   refused: `git` will fail honestly, or reach the host through the user's proxy.
///
/// Returns the addresses that were checked, so an https fetch can pin them
/// (`http.curloptResolve`) and `git` doesn't resolve the name again to something else.
pub fn check_destination(url: &GitUrl) -> Result<Vec<SocketAddr>, UrlError> {
    if url.scheme() == Scheme::File {
        return Ok(Vec::new());
    }
    if url.is_private_host() && !url.private_confirmed() {
        return Err(UrlError::PrivateNotConfirmed);
    }
    let Ok(found) = (url.host(), url.effective_port()).to_socket_addrs() else {
        return Ok(Vec::new());
    };
    let addrs: Vec<SocketAddr> = found.collect();
    check_addresses(&addrs, url.private_confirmed())?;
    Ok(addrs)
}

/// The decision for a list of resolved addresses. Pure, so it is tested without DNS.
pub fn check_addresses(addrs: &[SocketAddr], private_confirmed: bool) -> Result<(), UrlError> {
    let mut private = false;
    for a in addrs {
        match classify_ip(a.ip()) {
            AddrClass::Never => return Err(UrlError::SpecialAddress),
            AddrClass::Private => private = true,
            AddrClass::Public => {}
        }
    }
    if private && !private_confirmed {
        return Err(UrlError::PrivateNotConfirmed);
    }
    Ok(())
}

/// The `http.curloptResolve` value that pins `host:port` to the checked addresses, for an https
/// address with at least one. IPv6 addresses are bracketed, as curl expects.
pub fn curl_resolve(url: &GitUrl, addrs: &[SocketAddr]) -> Option<String> {
    if url.scheme() != Scheme::Https || addrs.is_empty() {
        return None;
    }
    let mut list: Vec<String> = Vec::new();
    for a in addrs {
        let text = match a.ip() {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        };
        if !list.contains(&text) {
            list.push(text);
        }
    }
    Some(format!(
        "{}:{}:{}",
        url.host(),
        url.effective_port(),
        list.join(",")
    ))
}

/// The class of an IP address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrClass {
    /// Reachable on the internet.
    Public,
    /// This computer or a private network: loopback, RFC 1918, carrier-grade NAT (100.64/10),
    /// IPv6 unique local (fc00::/7) and site-local, benchmark and IETF protocol ranges. Needs
    /// the user's "on my own computer or network" confirmation.
    Private,
    /// Never a Git server: unspecified, link-local (including cloud metadata services at
    /// 169.254.169.254), multicast, broadcast and reserved ranges. Always refused.
    Never,
}

/// Classifies an address. IPv4 addresses embedded in IPv6 (mapped `::ffff:a.b.c.d`,
/// compatible `::a.b.c.d`, NAT64 `64:ff9b::/96`, 6to4 `2002::/16`) are classified as the IPv4
/// address they carry, so wrapping cannot hide a private one.
pub fn classify_ip(ip: IpAddr) -> AddrClass {
    match ip {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => classify_v6(v6),
    }
}

fn classify_v4(ip: Ipv4Addr) -> AddrClass {
    let [a, b, c, _] = ip.octets();
    if a == 0 || ip.is_link_local() || ip.is_multicast() || ip.is_broadcast() || a >= 240 {
        return AddrClass::Never;
    }
    let private = ip.is_loopback()
        || ip.is_private()
        // Carrier-grade NAT, 100.64.0.0/10.
        || (a == 100 && (64..=127).contains(&b))
        // IETF protocol assignments, 192.0.0.0/24.
        || (a == 192 && b == 0 && c == 0)
        // Benchmarking, 198.18.0.0/15.
        || (a == 198 && (b == 18 || b == 19))
        // Documentation ranges never route on the internet either.
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113);
    if private {
        AddrClass::Private
    } else {
        AddrClass::Public
    }
}

fn classify_v6(ip: Ipv6Addr) -> AddrClass {
    if let Some(v4) = embedded_v4(ip) {
        return classify_v4(v4);
    }
    let seg = ip.segments();
    if ip.is_unspecified() || ip.is_multicast() || (seg[0] & 0xffc0) == 0xfe80 {
        return AddrClass::Never;
    }
    if ip.is_loopback()
        // Unique local, fc00::/7.
        || (seg[0] & 0xfe00) == 0xfc00
        // Deprecated site-local, fec0::/10.
        || (seg[0] & 0xffc0) == 0xfec0
        // Documentation, 2001:db8::/32.
        || (seg[0] == 0x2001 && seg[1] == 0x0db8)
    {
        return AddrClass::Private;
    }
    AddrClass::Public
}

/// The IPv4 address carried inside an IPv6 one, for the forms a resolver or an attacker can use
/// to point at an IPv4 host.
fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return Some(v4);
    }
    let seg = ip.segments();
    let low = |hi: u16, lo: u16| {
        let [a, b] = hi.to_be_bytes();
        let [c, d] = lo.to_be_bytes();
        Ipv4Addr::new(a, b, c, d)
    };
    // IPv4-compatible ::a.b.c.d (deprecated), but not :: or ::1.
    if seg[..6].iter().all(|s| *s == 0) && !(seg[6] == 0 && seg[7] <= 1) {
        return Some(low(seg[6], seg[7]));
    }
    // NAT64 well-known prefix, 64:ff9b::/96.
    if seg[0] == 0x0064 && seg[1] == 0xff9b && seg[2..6].iter().all(|s| *s == 0) {
        return Some(low(seg[6], seg[7]));
    }
    // 6to4, 2002:a.b.c.d::/48.
    if seg[0] == 0x2002 {
        return Some(low(seg[1], seg[2]));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(text: &str) -> AddrClass {
        classify_ip(text.parse().unwrap_or_else(|e| panic!("{text}: {e}")))
    }

    #[test]
    fn ipv4_classes() {
        for (ip, want) in [
            ("8.8.8.8", AddrClass::Public),
            ("140.82.112.3", AddrClass::Public),
            ("127.0.0.1", AddrClass::Private),
            ("127.255.255.254", AddrClass::Private),
            ("10.0.0.5", AddrClass::Private),
            ("172.16.0.1", AddrClass::Private),
            ("172.31.255.255", AddrClass::Private),
            ("172.32.0.1", AddrClass::Public),
            ("192.168.1.10", AddrClass::Private),
            ("100.64.0.1", AddrClass::Private),
            ("100.127.255.255", AddrClass::Private),
            ("100.128.0.1", AddrClass::Public),
            ("198.18.0.1", AddrClass::Private),
            ("192.0.2.1", AddrClass::Private),
            ("0.0.0.0", AddrClass::Never),
            ("0.1.2.3", AddrClass::Never),
            ("169.254.169.254", AddrClass::Never),
            ("224.0.0.1", AddrClass::Never),
            ("239.255.255.250", AddrClass::Never),
            ("240.0.0.1", AddrClass::Never),
            ("255.255.255.255", AddrClass::Never),
        ] {
            assert_eq!(class(ip), want, "{ip}");
        }
    }

    fn sock(text: &str) -> SocketAddr {
        SocketAddr::new(text.parse().expect("ip"), 443)
    }

    #[test]
    fn a_public_name_that_resolves_inward_needs_the_confirmation() {
        let public = [sock("140.82.112.3"), sock("2606:4700::1111")];
        assert_eq!(check_addresses(&public, false), Ok(()));
        // DNS rebinding or a public name pointing at a private address.
        for inward in [
            vec![sock("127.0.0.1")],
            vec![sock("140.82.112.3"), sock("10.0.0.7")],
            vec![sock("::1")],
            vec![sock("::ffff:192.168.1.1")],
            vec![sock("fd00::5")],
            vec![sock("100.64.1.1")],
        ] {
            assert_eq!(
                check_addresses(&inward, false),
                Err(UrlError::PrivateNotConfirmed),
                "{inward:?}"
            );
            assert_eq!(check_addresses(&inward, true), Ok(()), "{inward:?}");
        }
        // Never, even with the confirmation.
        for never in [
            vec![sock("169.254.169.254")],
            vec![sock("140.82.112.3"), sock("0.0.0.0")],
            vec![sock("fe80::1")],
            vec![sock("::ffff:169.254.169.254")],
            vec![sock("ff02::1")],
        ] {
            assert_eq!(
                check_addresses(&never, true),
                Err(UrlError::SpecialAddress),
                "{never:?}"
            );
        }
    }

    #[test]
    fn localhost_resolves_to_loopback_and_needs_the_confirmation() {
        let url = GitUrl::parse("https://localhost:1/o/r").expect("parse");
        assert_eq!(check_destination(&url), Err(UrlError::PrivateNotConfirmed));
        let confirmed = url.confirm_private();
        let addrs = check_destination(&confirmed).expect("confirmed");
        assert!(addrs.iter().all(|a| a.ip().is_loopback()), "{addrs:?}");
    }

    #[test]
    fn https_fetches_pin_the_checked_addresses() {
        let url = GitUrl::parse("https://git.example.com:8443/o/r").expect("parse");
        assert_eq!(
            curl_resolve(&url, &[sock("140.82.112.3"), sock("2606:4700::1111")]).as_deref(),
            Some("git.example.com:8443:140.82.112.3,[2606:4700::1111]")
        );
        assert_eq!(curl_resolve(&url, &[]), None);
        let ssh = GitUrl::parse("git@git.example.com:o/r").expect("parse");
        assert_eq!(
            curl_resolve(&ssh, &[sock("140.82.112.3")]),
            None,
            "ssh resolves on its own; documented in ADR-015"
        );
    }

    #[test]
    fn ipv6_classes_including_embedded_ipv4() {
        for (ip, want) in [
            ("2606:4700::1111", AddrClass::Public),
            ("::1", AddrClass::Private),
            ("::", AddrClass::Never),
            ("fc00::1", AddrClass::Private),
            ("fd12:3456::1", AddrClass::Private),
            ("fec0::1", AddrClass::Private),
            ("fe80::1", AddrClass::Never),
            ("febf::1", AddrClass::Never),
            ("ff02::1", AddrClass::Never),
            ("2001:db8::1", AddrClass::Private),
            // IPv4-mapped and friends are judged by the IPv4 address inside.
            ("::ffff:127.0.0.1", AddrClass::Private),
            ("::ffff:10.1.2.3", AddrClass::Private),
            ("::ffff:169.254.169.254", AddrClass::Never),
            ("::ffff:8.8.8.8", AddrClass::Public),
            ("::127.0.0.1", AddrClass::Private),
            ("64:ff9b::7f00:1", AddrClass::Private),
            ("64:ff9b::808:808", AddrClass::Public),
            ("2002:c0a8:0101::1", AddrClass::Private),
            ("2002:0808:0808::1", AddrClass::Public),
        ] {
            assert_eq!(class(ip), want, "{ip}");
        }
    }
}
