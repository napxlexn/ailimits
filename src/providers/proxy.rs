// providers/proxy.rs — the proxy set in Windows, for the app's HTTP clients.
//
// reqwest is built without its `system-proxy` feature: that would add
// registry imports to ailimits-auth.exe. The widget reads the same three
// values of the user's Internet Settings itself, with the registry calls
// its exe already makes (platform/win.rs), and only once the app has asked
// for it (`follow_windows_proxy`): code the CLI never calls is never linked
// into it. The HTTP(S)_PROXY variables still win, as they always did.

use std::sync::OnceLock;

/// Where the app's clients find a proxy beyond the environment: nowhere
/// until the app calls `follow_windows_proxy`.
static SOURCE: OnceLock<fn() -> Option<WindowsProxy>> = OnceLock::new();

/// The Windows proxy, read once: the first client built reads it, the rest
/// use the answer (a change applies after a restart, as for any program
/// that reads it at start).
static READ: OnceLock<Option<WindowsProxy>> = OnceLock::new();

/// Use the proxy set in Windows (Settings, Network, Proxy, "Use a proxy
/// server") for every client the providers build from now on, unless an
/// HTTP(S)_PROXY variable is set. A proxy script (PAC) is not followed.
/// The app calls this once at start, before it builds the providers.
pub fn follow_windows_proxy() {
    let _ = SOURCE.set(read_windows_proxy);
}

/// A proxy for the HTTPS requests the providers make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsProxy {
    /// "http://host:port": the proxy itself is reached over plain HTTP and
    /// tunnels TLS (CONNECT).
    pub url: String,
    /// Hosts that go direct, in reqwest's form ("a.example,.corp.example").
    pub bypass: String,
}

/// `builder` with the Windows proxy when one applies (see
/// `follow_windows_proxy`); unchanged otherwise.
pub(crate) fn with_windows_proxy(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    if env_proxy_set() {
        // reqwest reads the variables itself while no proxy is given.
        return builder;
    }
    let Some(source) = SOURCE.get() else {
        return builder;
    };
    let Some(proxy) = READ.get_or_init(source) else {
        return builder;
    };
    match reqwest::Proxy::https(&proxy.url) {
        Ok(p) => builder.proxy(p.no_proxy(reqwest::NoProxy::from_string(&proxy.bypass))),
        Err(_) => {
            tracing::warn!("the proxy set in Windows is not a usable proxy address");
            builder
        }
    }
}

/// Whether a proxy variable is set: then it is the one used.
fn env_proxy_set() -> bool {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .iter()
    .any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()))
}

/// The proxy the Internet Settings name, from their three values: whether
/// it is on (`ProxyEnable`), the server (`ProxyServer`: "host:port" for
/// every protocol, or "http=host:port;https=host:port"), and the hosts that
/// go direct (`ProxyOverride`: "<local>;*.corp.example;10.*"). None when it
/// is off or names no proxy for HTTPS.
pub(crate) fn proxy_from_settings(
    enabled: bool,
    server: &str,
    bypass: &str,
) -> Option<WindowsProxy> {
    if !enabled {
        return None;
    }
    let server = server.trim();
    let address = if server.contains('=') {
        // One proxy per protocol: HTTPS requests use the "https=" one; with
        // none, Windows sends them direct.
        server.split(';').find_map(|part| {
            let (scheme, address) = part.split_once('=')?;
            scheme
                .trim()
                .eq_ignore_ascii_case("https")
                .then(|| address.trim())
        })?
    } else {
        server
    };
    if address.is_empty() || address.chars().any(char::is_whitespace) {
        return None;
    }
    let url = if address.contains("://") {
        address.to_string()
    } else {
        format!("http://{address}")
    };
    let bypass = bypass
        .split(';')
        .map(str::trim)
        // "<local>" means names without a dot; every provider host has one.
        .filter(|entry| !entry.is_empty() && *entry != "<local>")
        .map(|entry| entry.strip_prefix('*').unwrap_or(entry))
        .collect::<Vec<_>>()
        .join(",");
    Some(WindowsProxy { url, bypass })
}

/// The current user's Internet Settings, read with the registry calls the
/// exe already makes.
#[cfg(target_os = "windows")]
fn read_windows_proxy() -> Option<WindowsProxy> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    };

    /// One value's bytes (at most 4 KB), or None.
    fn value(key: HKEY, name: PCWSTR) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; 4096];
        let mut len = buf.len() as u32;
        // SAFETY: `key` is open; the buffer and its length are ours.
        let read = unsafe {
            RegQueryValueExW(
                key,
                name,
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut len),
            )
        };
        if read.is_err() {
            return None;
        }
        buf.truncate((len as usize).min(buf.len()));
        Some(buf)
    }

    /// REG_SZ bytes as text, without the trailing NULs.
    fn text(bytes: &[u8]) -> String {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&u| u != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }

    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings"),
            None,
            KEY_QUERY_VALUE,
            &mut key,
        )
        .is_err()
        {
            return None;
        }
        let enabled = value(key, w!("ProxyEnable"))
            .and_then(|b| {
                b.get(..4)
                    .map(|d| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
            })
            .is_some_and(|d| d != 0);
        let server = value(key, w!("ProxyServer")).map(|b| text(&b));
        let bypass = value(key, w!("ProxyOverride")).map(|b| text(&b));
        let _ = RegCloseKey(key);
        let proxy = proxy_from_settings(
            enabled,
            server.as_deref().unwrap_or_default(),
            bypass.as_deref().unwrap_or_default(),
        );
        if proxy.is_some() {
            // Not the address: a user may have typed credentials into it.
            tracing::info!("using the proxy set in Windows");
        }
        proxy
    }
}

#[cfg(not(target_os = "windows"))]
fn read_windows_proxy() -> Option<WindowsProxy> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy(url: &str, bypass: &str) -> Option<WindowsProxy> {
        Some(WindowsProxy {
            url: url.to_string(),
            bypass: bypass.to_string(),
        })
    }

    #[test]
    fn the_settings_name_the_proxy_for_https() {
        assert_eq!(
            proxy_from_settings(true, "proxy.corp.example:8080", ""),
            proxy("http://proxy.corp.example:8080", "")
        );
        assert_eq!(
            proxy_from_settings(
                true,
                "http=web:80;https=secure:8443;ftp=f:21",
                "<local>;*.corp.example;10.1.2.3"
            ),
            proxy("http://secure:8443", ".corp.example,10.1.2.3")
        );
        assert_eq!(
            proxy_from_settings(true, "http://proxy:3128", "*.a.example; b.example"),
            proxy("http://proxy:3128", ".a.example,b.example")
        );
    }

    #[test]
    fn no_proxy_when_off_or_none_for_https() {
        assert_eq!(proxy_from_settings(false, "proxy:8080", ""), None);
        assert_eq!(proxy_from_settings(true, "", ""), None);
        assert_eq!(proxy_from_settings(true, "  ", ""), None);
        assert_eq!(proxy_from_settings(true, "http=web:80;ftp=f:21", ""), None);
        assert_eq!(proxy_from_settings(true, "https=", ""), None);
        assert_eq!(proxy_from_settings(true, "pro xy:80", ""), None);
    }

    /// Until the app asks for it, a client is built exactly as before.
    #[test]
    fn nothing_changes_until_the_app_asks() {
        if SOURCE.get().is_none() {
            let _client = with_windows_proxy(reqwest::Client::builder())
                .build()
                .expect("a client");
            assert!(READ.get().is_none(), "nothing was read");
        }
    }
}
