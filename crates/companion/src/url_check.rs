//! Base URL rules for provider profiles (ADR-020): https, or http for loopback only; no
//! userinfo, query or fragment.

use url::{Host, Url};

use crate::error::CompanionError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUrl {
    /// Without a trailing slash.
    pub url: String,
    /// `host` or `host:port`, for "Sends to <host>".
    pub host: String,
    pub loopback: bool,
}

fn bad(msg: &str) -> CompanionError {
    CompanionError::invalid_input(msg.to_owned())
}

pub fn check_base_url(input: &str) -> Result<CheckedUrl, CompanionError> {
    let input = input.trim();
    if input.is_empty() || input.len() > 2048 || input.chars().any(char::is_control) {
        return Err(bad("Enter a base URL such as https://example.com/v1."));
    }
    let u = Url::parse(input).map_err(|_| bad("That isn't a valid URL."))?;
    if !u.username().is_empty() || u.password().is_some() {
        return Err(bad("The URL must not contain a user name or password."));
    }
    if u.query().is_some() || u.fragment().is_some() {
        return Err(bad("The URL must not contain a query or fragment."));
    }
    let host = u.host().ok_or_else(|| bad("The URL needs a host name."))?;
    let loopback = match &host {
        Host::Domain(d) => d.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(a) => a.octets() == [127, 0, 0, 1],
        Host::Ipv6(a) => a.is_loopback(),
    };
    match u.scheme() {
        "https" => {}
        "http" if loopback => {}
        "http" => {
            return Err(bad(
                "http is only allowed for this computer (localhost, 127.0.0.1 or ::1). Use https.",
            ));
        }
        _ => return Err(bad("The URL must start with https://.")),
    }
    let host_text = match u.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    };
    Ok(CheckedUrl {
        url: u.as_str().trim_end_matches('/').to_owned(),
        host: host_text,
        loopback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_and_loopback_http() {
        assert_eq!(
            check_base_url("https://api.example.com/v1/").unwrap().url,
            "https://api.example.com/v1"
        );
        for u in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080",
            "http://[::1]:8080/v1",
        ] {
            assert!(check_base_url(u).unwrap().loopback, "{u}");
        }
        assert_eq!(
            check_base_url("https://x.example:8443").unwrap().host,
            "x.example:8443"
        );
    }

    #[test]
    fn refuses_everything_else() {
        for u in [
            "http://example.com",
            "http://192.168.1.5",
            "http://127.0.0.2",
            "ftp://example.com",
            "https://user:pw@example.com",
            "https://user@example.com",
            "https://example.com/v1?x=1",
            "https://example.com/#f",
            "example.com",
            "",
            "https://",
        ] {
            assert!(check_base_url(u).is_err(), "{u}");
        }
    }

    #[test]
    fn errors_do_not_echo_the_input() {
        // secretlint-disable-next-line -- fake credential: the test checks it is refused
        let e = check_base_url("https://user:SECRETPW@example.com").unwrap_err();
        assert!(!e.message.contains("SECRETPW"));
    }
}
