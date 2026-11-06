//! What kind of network an address is on (decision 17, ADR-015 amendment).
//!
//! Lullshift lets a marketplace live on the user's own computer or network only after the user
//! confirms it. This module says which addresses need that confirmation and which can never be
//! a Git server at all.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use crate::error::UrlError;
use crate::validate::{GitUrl, Scheme};

/// Looks a host name up. The system resolver in the app; a stub in tests.
pub type Resolve = fn(&str, u16) -> std::io::Result<Vec<SocketAddr>>;

/// The system resolver (`getaddrinfo`).
pub fn system_resolve(host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
    Ok((host, port).to_socket_addrs()?.collect())
}

/// What [`check_destination`] found out about where a fetch would go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// A `file://` repository (tests only): no network at all.
    Local,
    /// The name resolved, and every address it resolved to is allowed. Never empty.
    Resolved(Vec<SocketAddr>),
    /// The name didn't resolve here (no network, or only a proxy can resolve it), or resolved
    /// to nothing. Nothing was checked, so the caller decides: an https fetch without a proxy
    /// is refused (security review MEDIUM-1), because `git` would resolve the name itself,
    /// unpinned.
    Unresolved,
}

/// Where a fetch may go, checked just before it starts (decisions 17 and 18.8).
///
/// - A private host in the address needs the user's confirmation.
/// - The host name is resolved with the system resolver; if any address it resolves to can
///   never be a Git server the fetch is refused, and if any is private the fetch needs the same
///   confirmation as a private address. IPv4 inside IPv6 is judged as IPv4.
/// - A name that doesn't resolve is [`Destination::Unresolved`].
///
/// The addresses in [`Destination::Resolved`] are what an https fetch pins
/// (`http.curloptResolve`) and what the redirect probe connects to, so neither resolves the
/// name again to something else.
pub fn check_destination(url: &GitUrl) -> Result<Destination, UrlError> {
    check_destination_with(url, system_resolve)
}

/// [`check_destination`] with a given resolver.
pub fn check_destination_with(url: &GitUrl, resolve: Resolve) -> Result<Destination, UrlError> {
    if url.scheme() == Scheme::File {
        return Ok(Destination::Local);
    }
    if url.is_private_host() && !url.private_confirmed() {
        return Err(UrlError::PrivateNotConfirmed);
    }
    let addrs = match resolve(url.host(), url.effective_port()) {
        Ok(addrs) if !addrs.is_empty() => addrs,
        _ => return Ok(Destination::Unresolved),
    };
    check_addresses(&addrs, url.private_confirmed())?;
    Ok(Destination::Resolved(addrs))
}

/// The proxy variables curl (and so Git's https transport) reads for an https address. A
/// proxy resolves the target name itself, so neither the address check nor the pin can bind
/// the connection behind one (ADR-015 amendment).
const PROXY_ENV: &[&str] = &["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"];

/// True if the environment configures a proxy for https. `get` reads one variable.
pub fn env_proxy_configured(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    PROXY_ENV
        .iter()
        .any(|k| get(k).is_some_and(|v| !v.is_empty()))
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
/// translated `::ffff:0:a.b.c.d`, compatible `::a.b.c.d`, NAT64 `64:ff9b::/96`, 6to4
/// `2002::/16`) are classified as the IPv4 address they carry, so wrapping cannot hide a
/// private one; NAT64 local-use `64:ff9b:1::/48` is at best Private. Teredo, discard-only and
/// ORCHID addresses are never allowed.
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
    // NAT64 local-use prefix, 64:ff9b:1::/48 (RFC 8215). The IPv4 address is judged as usual
    // (taken from the last 32 bits, where a /96 deployment puts it), and the result is never
    // better than Private: the prefix is only routed inside an operator's own network, and with
    // a shorter prefix the IPv4 bits sit elsewhere, so the last 32 bits alone can't clear it.
    if seg[0] == 0x0064 && seg[1] == 0xff9b && seg[2] == 0x0001 {
        let [a, b] = seg[6].to_be_bytes();
        let [c, d] = seg[7].to_be_bytes();
        return match classify_v4(Ipv4Addr::new(a, b, c, d)) {
            AddrClass::Never => AddrClass::Never,
            _ => AddrClass::Private,
        };
    }
    if ip.is_unspecified()
        || ip.is_multicast()
        || (seg[0] & 0xffc0) == 0xfe80
        // Discard-only, 100::/64 (RFC 6666): traffic to it is dropped by design.
        || (seg[0] == 0x0100 && seg[1..4].iter().all(|s| *s == 0))
        // Teredo, 2001::/32 (RFC 4380): a tunnel that carries the client's IPv4 address
        // (obfuscated) and the server's, so it can lead to any IPv4 host, private ones
        // included. No Git server is reached over Teredo; never allowed.
        || (seg[0] == 0x2001 && seg[1] == 0x0000)
        // ORCHID and ORCHIDv2, 2001:10::/28 and 2001:20::/28 (RFC 4843, RFC 7343): host
        // identifiers that must never appear as network addresses.
        || (seg[0] == 0x2001 && matches!(seg[1] & 0xfff0, 0x0010 | 0x0020))
    {
        return AddrClass::Never;
    }
    if ip.is_loopback()
        // Unique local, fc00::/7.
        || (seg[0] & 0xfe00) == 0xfc00
        // Deprecated site-local, fec0::/10.
        || (seg[0] & 0xffc0) == 0xfec0
        // Documentation, 2001:db8::/32.
        || (seg[0] == 0x2001 && seg[1] == 0x0db8)
        // The rest of the IETF protocol assignments, 2001::/23 (benchmarking 2001:2::/48,
        // AMT, AS112, anycast services): like IPv4's 192.0.0.0/24 and 198.18.0.0/15, not where
        // a public Git server lives, so they need the user's confirmation.
        || (seg[0] == 0x2001 && seg[1] < 0x0200)
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
    // IPv4-translated (SIIT), ::ffff:0:a.b.c.d (RFC 2765).
    if seg[..4].iter().all(|s| *s == 0) && seg[4] == 0xffff && seg[5] == 0 {
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
        let Destination::Resolved(addrs) = check_destination(&confirmed).expect("confirmed") else {
            panic!("localhost resolves");
        };
        assert!(addrs.iter().all(|a| a.ip().is_loopback()), "{addrs:?}");
    }

    fn unresolvable(_: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
        Err(std::io::ErrorKind::NotFound.into())
    }

    fn resolves_to_nothing(_: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
        Ok(Vec::new())
    }

    #[test]
    fn a_name_that_does_not_resolve_is_reported_unchecked() {
        let url = GitUrl::parse("https://git.example.com/o/r").expect("parse");
        assert_eq!(
            check_destination_with(&url, unresolvable),
            Ok(Destination::Unresolved)
        );
        // An empty answer is not "checked and fine": nothing could be pinned.
        assert_eq!(
            check_destination_with(&url, resolves_to_nothing),
            Ok(Destination::Unresolved)
        );
        // The text rule still comes first.
        let private = GitUrl::parse("https://gitserver/o/r").expect("parse");
        assert_eq!(
            check_destination_with(&private, unresolvable),
            Err(UrlError::PrivateNotConfirmed)
        );
    }

    #[test]
    fn https_proxy_variables_are_recognised() {
        use std::ffi::OsString;
        assert!(!env_proxy_configured(|_| None));
        assert!(!env_proxy_configured(|_| Some(OsString::new())));
        for name in ["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"] {
            assert!(
                env_proxy_configured(|k| (k == name).then(|| OsString::from("http://p:3128"))),
                "{name}"
            );
        }
        // http_proxy is for plain http only; curl ignores it for https.
        assert!(!env_proxy_configured(
            |k| (k == "http_proxy").then(|| OsString::from("http://p:3128"))
        ));
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

    /// Security review LOW-3: more IPv6 forms that carry or hide an inward address.
    #[test]
    fn more_ipv6_wrappers_and_special_ranges() {
        for (ip, want) in [
            // IPv4-translated (SIIT), unwrapped.
            ("::ffff:0:127.0.0.1", AddrClass::Private),
            ("::ffff:0:a9fe:a9fe", AddrClass::Never),
            ("::ffff:0:808:808", AddrClass::Public),
            // NAT64 local-use: never better than Private, Never if the IPv4 inside is.
            ("64:ff9b:1::808:808", AddrClass::Private),
            ("64:ff9b:1::a00:5", AddrClass::Private),
            ("64:ff9b:1::a9fe:a9fe", AddrClass::Never),
            ("64:ff9b:1:ffff::e000:1", AddrClass::Never),
            // Next to it, but outside the /48: an ordinary global address.
            ("64:ff9b:2::808:808", AddrClass::Public),
            // Teredo, 2001::/32: never.
            ("2001::1", AddrClass::Never),
            ("2001:0:4136:e378:8000:63bf:3fff:fdd2", AddrClass::Never),
            ("2001:0:ffff:ffff:ffff:ffff:ffff:ffff", AddrClass::Never),
            // Discard-only, 100::/64: never; just outside it is not special.
            ("100::1", AddrClass::Never),
            ("100::ffff:ffff:ffff:ffff", AddrClass::Never),
            ("100:0:0:1::1", AddrClass::Public),
            // ORCHID and ORCHIDv2: never.
            ("2001:10::1", AddrClass::Never),
            ("2001:1f:ffff::1", AddrClass::Never),
            ("2001:20::1", AddrClass::Never),
            ("2001:2f:ffff::1", AddrClass::Never),
            // The rest of 2001::/23 needs the confirmation.
            ("2001:1::1", AddrClass::Private),
            ("2001:2::1", AddrClass::Private),
            ("2001:3::1", AddrClass::Private),
            ("2001:4:112::1", AddrClass::Private),
            ("2001:1ff:ffff::1", AddrClass::Private),
            // Just past 2001::/23 is ordinary global unicast.
            ("2001:200::1", AddrClass::Public),
            ("2001:4860:4860::8888", AddrClass::Public),
        ] {
            assert_eq!(class(ip), want, "{ip}");
        }
    }
}
