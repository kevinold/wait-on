//! The four pure parsers the #245 property suite covers, transcribed from their JS regexes
//! (lib/resources.js, bin/wait-on `parseInterval`) so the differential can hold them equal.
//! Std only, and every slice is at a `char` boundary, so no input can panic.

/// `PREFIX_RE` split plus the `createResource$` type routing.
#[derive(Debug, PartialEq)]
pub struct Prefix {
    pub prefix: String,
    pub rest: String,
    pub kind: String,
}

/// `HOST_PORT_RE` split; `port` stays a string like the JS match group.
#[derive(Debug, PartialEq)]
pub struct HostPort {
    pub host: String,
    pub port: String,
}

/// `parseInterval` outcome: a number, the input back when the pattern misses, or
/// `undefined` when the unit matched only case-insensitively.
#[derive(Debug, PartialEq)]
pub enum Interval {
    Number(f64),
    Unchanged(String),
    Undefined,
}

/// `HTTP_UNIX_RE` / `HTTP_UNIX_LEGACY_RE` split.
#[derive(Debug, PartialEq)]
pub struct HttpUnix {
    pub socket_path: String,
    pub request_path: String,
}

/// JS `.` and a non-multiline `$` exclude exactly these.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn one_line(s: &str) -> bool {
    !s.chars().any(is_line_terminator)
}

fn ascii_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `^((https?-get|https?|tcp|socket|file|command):)(.+)$`, typed like `createResource$`.
pub fn parse_prefix(resource: &str) -> Prefix {
    const SCHEMES: [(&str, &str); 8] = [
        ("https-get:", "http"),
        ("http-get:", "http"),
        ("https:", "http"),
        ("http:", "http"),
        ("tcp:", "tcp"),
        ("socket:", "socket"),
        ("file:", "file"),
        ("command:", "command"),
    ];
    let hit = SCHEMES.iter().find_map(|&(p, kind)| {
        let rest = resource.strip_prefix(p)?;
        (!rest.is_empty() && one_line(rest)).then_some((p, rest, kind))
    });
    let (prefix, rest, kind) = hit.unwrap_or(("", resource, "file"));
    Prefix {
        prefix: prefix.into(),
        rest: rest.into(),
        kind: kind.into(),
    }
}

/// `^(?:\[([^\]]+)\]:|([^:]*):)?(\d+)$`; an empty host reads `localhost`.
pub fn parse_host_port(s: &str) -> Option<HostPort> {
    let bracket = s.strip_prefix('[').and_then(|t| {
        let end = t.find(']')?;
        let port = t[end + 1..].strip_prefix(':')?;
        (end > 0 && ascii_digits(port)).then_some((&t[..end], port))
    });
    let (host, port) = match (bracket, s.split_once(':')) {
        (Some(hp), _) => hp,
        (None, Some(hp)) => hp,
        (None, None) => ("", s),
    };
    ascii_digits(port).then(|| HostPort {
        host: if host.is_empty() { "localhost" } else { host }.into(),
        port: port.into(),
    })
}

/// bin/wait-on `parseInterval`: `/^([\d.]+)(|ms|s|m|h)$/i`, then a case-sensitive switch.
pub fn parse_interval(arg: &str) -> Interval {
    let split = arg
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(arg.len());
    let (run, unit) = arg.split_at(split);
    let known = ["", "ms", "s", "m", "h"];
    if run.is_empty() || !known.iter().any(|u| unit.eq_ignore_ascii_case(u)) {
        return Interval::Unchanged(arg.into());
    }
    let value = parse_float(run);
    let ms = match unit {
        "" | "ms" => value,
        "s" => value * 1000.0,
        "m" => value * 1000.0 * 60.0,
        "h" => value * 1000.0 * 60.0 * 60.0,
        _ => return Interval::Undefined,
    };
    Interval::Number(ms.floor())
}

/// JS `parseFloat` over a `[0-9.]+` run: the longest `digits[.digits]` prefix, else NaN.
fn parse_float(run: &str) -> f64 {
    let int_end = run.find('.').unwrap_or(run.len());
    let (int, tail) = run.split_at(int_end);
    let frac = tail.get(1..).unwrap_or("");
    let frac = &frac[..frac.find('.').unwrap_or(frac.len())];
    if int.is_empty() && frac.is_empty() {
        return f64::NAN;
    }
    format!("0{int}.{frac}0").parse().unwrap_or(f64::NAN)
}

/// `createHTTP$` split: first `-get:` -> `:`, then `HTTP_UNIX_RE` (lazy socket part),
/// else `HTTP_UNIX_LEGACY_RE`.
pub fn parse_http_unix(resource: &str) -> Option<HttpUnix> {
    let raw = resource.replacen("-get:", ":", 1);
    let body = raw.strip_prefix("http://unix:")?;
    let split = |i: usize| (&body[..i], &body[i + 1..]);
    let lazy = body
        .match_indices(':')
        .map(|(i, _)| split(i))
        .find(|(sock, req)| {
            !sock.is_empty()
                && one_line(sock)
                && one_line(req)
                && ["https://", "http://", "/"]
                    .iter()
                    .any(|p| req.starts_with(p))
        });
    let legacy = || {
        let (sock, req) = body.split_once(':')?;
        (!sock.is_empty() && !req.is_empty() && one_line(req)).then_some((sock, req))
    };
    let (sock, req) = lazy.or_else(legacy)?;
    Some(HttpUnix {
        socket_path: sock.into(),
        request_path: req.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(p: &str, rest: &str, kind: &str) -> Prefix {
        Prefix {
            prefix: p.into(),
            rest: rest.into(),
            kind: kind.into(),
        }
    }

    fn hp(host: &str, port: &str) -> Option<HostPort> {
        Some(HostPort {
            host: host.into(),
            port: port.into(),
        })
    }

    fn hu(socket_path: &str, request_path: &str) -> Option<HttpUnix> {
        Some(HttpUnix {
            socket_path: socket_path.into(),
            request_path: request_path.into(),
        })
    }

    #[test]
    fn prefix_matches_js_golden_vectors() {
        for (input, p, rest, kind) in [
            (
                "tcp:my.server.com:3000",
                "tcp:",
                "my.server.com:3000",
                "tcp",
            ),
            (
                "http://foo.com:8000/bar",
                "http:",
                "//foo.com:8000/bar",
                "http",
            ),
            (
                "https-get:https://my/bar",
                "https-get:",
                "https://my/bar",
                "http",
            ),
            (
                "http-get:http://m.com:90/foo",
                "http-get:",
                "http://m.com:90/foo",
                "http",
            ),
            ("socket:/path/sock", "socket:", "/path/sock", "socket"),
            ("command:pg_isready", "command:", "pg_isready", "command"),
            ("file:/tmp/x", "file:", "/tmp/x", "file"),
            ("/tmp/plain", "", "/tmp/plain", "file"),
            ("tcp:", "", "tcp:", "file"),
            ("", "", "", "file"),
            ("tcp:ünïcödé", "tcp:", "ünïcödé", "tcp"),
        ] {
            assert_eq!(parse_prefix(input), prefix(p, rest, kind), "{input:?}");
        }
    }

    #[test]
    fn prefix_rest_with_a_line_terminator_is_a_file() {
        for input in ["tcp:a\nb", "tcp:a\u{2028}b", "http-get:x\r"] {
            assert_eq!(parse_prefix(input), prefix("", input, "file"), "{input:?}");
        }
    }

    #[test]
    fn host_port_matches_js_golden_vectors() {
        for (input, host, port) in [
            ("3000", "localhost", "3000"),
            ("host.example:8080", "host.example", "8080"),
            ("[::1]:8080", "::1", "8080"),
            ("[2001:db8::1]:443", "2001:db8::1", "443"),
            (":3000", "localhost", "3000"),
            ("[]:80", "[]", "80"),
            ("[a\nb]:80", "a\nb", "80"),
            ("[a:80", "[a", "80"),
            ("ünïcödé:80", "ünïcödé", "80"),
        ] {
            assert_eq!(parse_host_port(input), hp(host, port), "{input:?}");
        }
    }

    #[test]
    fn host_port_rejects_what_js_rejects() {
        for input in [
            "",
            "abc",
            "3000x",
            "host:",
            "host:port",
            "a:b:80",
            ":",
            "[::1]",
            "80:",
            "3000\n",
            "3000\r",
            "٣٠",
        ] {
            assert_eq!(parse_host_port(input), None, "{input:?}");
        }
    }

    #[test]
    fn interval_matches_js_numbers() {
        for (input, ms) in [
            ("250", 250.0),
            ("250ms", 250.0),
            ("2s", 2000.0),
            ("1m", 60000.0),
            ("1h", 3600000.0),
            ("2.5s", 2500.0),
            (".5s", 500.0),
            ("0", 0.0),
            ("1.2.3s", 1200.0),
            ("5.", 5.0),
            ("1.5m", 90000.0),
            ("0.001h", 3600.0),
        ] {
            assert_eq!(parse_interval(input), Interval::Number(ms), "{input:?}");
        }
    }

    #[test]
    fn interval_edge_values_match_js() {
        assert!(matches!(parse_interval(".s"), Interval::Number(n) if n.is_nan()));
        let huge = format!("1{}h", "0".repeat(400));
        assert_eq!(parse_interval(&huge), Interval::Number(f64::INFINITY));
    }

    #[test]
    fn interval_uppercase_unit_is_undefined() {
        for input in ["2S", "2M", "2H", "5MS", "5mS", "5S"] {
            assert_eq!(parse_interval(input), Interval::Undefined, "{input:?}");
        }
    }

    #[test]
    fn interval_miss_returns_the_input() {
        for input in ["abc", "12sec", "2ſ", "250\n", ""] {
            assert_eq!(
                parse_interval(input),
                Interval::Unchanged(input.into()),
                "{input:?}"
            );
        }
    }

    #[test]
    fn http_unix_matches_js_golden_vectors() {
        for (input, sock, path) in [
            (
                "http://unix:/path/to/sock:/foo/bar",
                "/path/to/sock",
                "/foo/bar",
            ),
            (
                "http-get://unix:/path/to/sock:/foo/bar",
                "/path/to/sock",
                "/foo/bar",
            ),
            (
                "http://unix:/var/run/app.sock:http://localhost/health",
                "/var/run/app.sock",
                "http://localhost/health",
            ),
            (
                r"http://unix:\\?\pipe\C:\app\sock:/status",
                r"\\?\pipe\C:\app\sock",
                "/status",
            ),
            ("http://unix:/sock:foo", "/sock", "foo"),
            ("http-get://unix:/a-get:/b:/c", "/a-get", "/b:/c"),
            ("http://unix:/a\nb:/c", "/a\nb", "/c"),
            ("http://unix:/a\u{2029}:b", "/a\u{2029}", "b"),
            ("http://unix:::/x", ":", "/x"),
        ] {
            assert_eq!(parse_http_unix(input), hu(sock, path), "{input:?}");
        }
    }

    #[test]
    fn http_unix_rejects_what_js_rejects() {
        for input in [
            "https://unix:/s:/p",
            "http://foo.com/bar",
            "tcp:host:3000",
            "file:/x",
            "http://unix:/a:/b\n",
            "http://unix:",
            "http://unix::",
        ] {
            assert_eq!(parse_http_unix(input), None, "{input:?}");
        }
    }

    #[test]
    fn parsers_never_panic_on_odd_input() {
        let long = ":/".repeat(5000);
        for input in [
            "",
            ":",
            "[",
            "]",
            "\n",
            "http://unix:",
            "http://unix::",
            "-get:",
            &long,
            "ünïcödé:80",
            "tcp:ünïcödé",
            "http://unix:ü:ü",
            "[ü]:ü",
            "ü.üms",
        ] {
            parse_prefix(input);
            parse_host_port(input);
            parse_interval(input);
            parse_http_unix(input);
        }
    }
}
