use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Context;

use crate::PORT;

/// The TCP port from `PORT`. Unset or empty means the default 3909.
pub fn listen_port(raw: Option<&str>) -> anyhow::Result<u16> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(PORT),
        Some(value) => value
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| anyhow::anyhow!("PORT must be a TCP port from 1 to 65535: {value}")),
    }
}

/// `HOST` 为空或未设置时监听回环地址。容器镜像用 `ENV HOST=0.0.0.0` 给出镜像内默认值。
pub fn listen_addr(host: Option<&str>, port: u16) -> anyhow::Result<SocketAddr> {
    let ip = match host.map(str::trim).filter(|value| !value.is_empty()) {
        None => IpAddr::V4(Ipv4Addr::LOCALHOST),
        Some(value) => value
            .parse()
            .with_context(|| format!("HOST 无法解析为 IP 地址: {value}"))?,
    };
    Ok(SocketAddr::new(ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_host_binds_loopback() {
        let addr = listen_addr(None, PORT).unwrap();
        assert_eq!(addr, SocketAddr::from(([127, 0, 0, 1], PORT)));
        assert_eq!(listen_addr(Some(""), PORT).unwrap(), addr);
        assert_eq!(listen_addr(Some("  "), PORT).unwrap(), addr);
    }

    #[test]
    fn port_defaults_to_3909_and_can_be_set() {
        assert_eq!(PORT, 3909);
        assert_eq!(listen_port(None).unwrap(), 3909);
        assert_eq!(listen_port(Some("")).unwrap(), 3909);
        assert_eq!(listen_port(Some(" ")).unwrap(), 3909);
        assert_eq!(listen_port(Some("8080")).unwrap(), 8080);
        assert_eq!(listen_port(Some(" 65535 ")).unwrap(), 65535);
        assert_eq!(
            listen_addr(None, listen_port(Some("4000")).unwrap()).unwrap(),
            SocketAddr::from(([127, 0, 0, 1], 4000))
        );
    }

    #[test]
    fn rejects_an_invalid_port() {
        for raw in ["0", "65536", "-1", "http", "80a", "3909.0"] {
            let error = listen_port(Some(raw)).unwrap_err().to_string();
            assert!(
                error.contains("PORT") && error.contains(raw),
                "{raw}: {error}"
            );
        }
    }

    #[test]
    fn explicit_host_is_used() {
        let addr = listen_addr(Some("0.0.0.0"), PORT).unwrap();
        assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(addr.port(), PORT);
    }

    #[test]
    fn rejects_unparsed_host() {
        let error = listen_addr(Some("everywhere"), PORT).unwrap_err();
        assert!(error.to_string().contains("everywhere"));
    }
}
