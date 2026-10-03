use napi::bindgen_prelude::{Either3, Undefined};
use napi_derive::napi;
use wait_on_core::parse::Interval;

mod http;
pub mod wait;

#[napi]
pub fn version() -> String {
    wait_on_core::version().to_string()
}

#[napi]
pub fn noop() {}

// Parsers, exposed only for the differential in test/parser-properties.mocha.js;
// each shape deep-equals its `nodeParsers` counterpart.

#[napi(object)]
pub struct ParsedPrefix {
    pub prefix: String,
    pub rest: String,
    #[napi(js_name = "type")]
    pub kind: String,
}

#[napi(object)]
pub struct ParsedHostPort {
    pub host: String,
    pub port: String,
}

#[napi(object)]
pub struct ParsedHttpUnix {
    pub socket_path: String,
    pub request_path: String,
}

#[napi]
pub fn parse_prefix(resource: String) -> ParsedPrefix {
    let p = wait_on_core::parse::parse_prefix(&resource);
    ParsedPrefix {
        prefix: p.prefix,
        rest: p.rest,
        kind: p.kind,
    }
}

/// `{ host, port } | null`.
#[napi]
pub fn parse_host_port(s: String) -> Option<ParsedHostPort> {
    wait_on_core::parse::parse_host_port(&s).map(|hp| ParsedHostPort {
        host: hp.host,
        port: hp.port,
    })
}

/// `number | string | undefined`; NaN crosses as NaN, the uppercase-unit case as `undefined`.
#[napi]
pub fn parse_interval(arg: String) -> Either3<f64, String, Undefined> {
    match wait_on_core::parse::parse_interval(&arg) {
        Interval::Number(n) => Either3::A(n),
        Interval::Unchanged(s) => Either3::B(s),
        Interval::Undefined => Either3::C(()),
    }
}

/// `{ socketPath, requestPath } | null`.
#[napi]
pub fn parse_http_unix(resource: String) -> Option<ParsedHttpUnix> {
    wait_on_core::parse::parse_http_unix(&resource).map(|h| ParsedHttpUnix {
        socket_path: h.socket_path,
        request_path: h.request_path,
    })
}
