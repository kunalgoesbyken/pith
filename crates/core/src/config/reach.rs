use super::Config;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Reach {
    #[default]
    ThisPc,
    Server(String),
}

struct Remote<'a> {
    secure: bool,
    host: &'a str,
}

fn remote(url: &str) -> Option<Remote<'_>> {
    if url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return None;
    }
    let (secure, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        (None, None) => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let host = match host.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => host.split(':').next().unwrap_or_default(),
    };
    (!host.is_empty()).then_some(Remote { secure, host })
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

pub(super) fn is_private_remote(url: &str) -> bool {
    remote(url).is_some_and(|remote| remote.secure || is_loopback(remote.host))
}

impl Config {
    pub fn reach(&self) -> Reach {
        match self.remote_url.as_deref().and_then(remote) {
            Some(remote) if !is_loopback(remote.host) => Reach::Server(remote.host.to_string()),
            _ => Reach::ThisPc,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_names_the_server_only_when_text_leaves_the_pc() {
        for (url, expected) in [
            (None, Reach::ThisPc),
            (Some("http://localhost:8080/v1"), Reach::ThisPc),
            (Some("http://127.0.0.1:8080"), Reach::ThisPc),
            (Some("https://[::1]/v1"), Reach::ThisPc),
            (Some("https://LOCALHOST"), Reach::ThisPc),
            (
                Some("https://example.com/v1"),
                Reach::Server("example.com".into()),
            ),
            (
                Some("https://key@api.example.com:8443/v1"),
                Reach::Server("api.example.com".into()),
            ),
            (
                Some("https://[2001:db8::1]/v1"),
                Reach::Server("2001:db8::1".into()),
            ),
            (
                Some("https://localhost.example.com"),
                Reach::Server("localhost.example.com".into()),
            ),
        ] {
            let config = Config {
                remote_url: url.map(str::to_string),
                ..Config::default()
            };
            assert_eq!(config.reach(), expected, "{url:?}");
        }
    }
}
