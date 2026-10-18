//! The errors of Go's `url.Parse` for the URI of a Docker action, which the reference's
//! `checkDockerAction` puts in its message.
//!
//! Ported from Go 1.26.4, which builds the reference: `Parse`, `parse`, `getScheme`,
//! `parseAuthority`, `parseHost`, `unescape`, and `shouldEscape` in `net/url/url.go`, and
//! `ParseAddr` with its IPv4 parser in `net/netip/netip.go`. Only the paths that a URI
//! starting with `docker://` can take are ported. The error texts are copied verbatim.
//!
//! Percent-decoding can produce bytes that are not UTF-8, which Go quotes byte by byte, so the
//! parsers work on bytes.

use workflint_diagnostics::quote::quote_bytes;

/// Returns the text of the error that Go's `url.Parse` returns for `uri`, a value that starts
/// with `docker://`, or `None` if it parses.
pub(super) fn parse_error(uri: &str) -> Option<String> {
    let raw = uri.as_bytes();
    let (before_fragment, fragment) = cut(raw, b'#');
    if let Err(err) = parse(before_fragment) {
        return Some(format!("parse {}: {err}", quote_bytes(before_fragment)));
    }
    match fragment {
        Some(fragment) if !fragment.is_empty() => unescape(fragment, Mode::Fragment)
            .err()
            .map(|err| format!("parse {}: {err}", quote_bytes(raw))),
        _ => None,
    }
}

/// Go's `parse(rawURL, false)`, for a URI whose scheme is not `http` or `https`.
fn parse(raw: &[u8]) -> Result<(), String> {
    if raw.iter().any(|&byte| byte < b' ' || byte == 0x7f) {
        return Err("net/url: invalid control character in URL".to_owned());
    }
    if raw == b"*" {
        return Ok(());
    }
    let (has_scheme, rest) = scheme(raw)?;
    // Go keeps a final `?` apart when it is the only one, which leaves the same text before it.
    let rest = cut(rest, b'?').0;
    if !rest.starts_with(b"/") {
        if has_scheme {
            // An opaque URI, which is not checked further.
            return Ok(());
        }
        if cut(rest, b'/').0.contains(&b':') {
            return Err("first path segment in URL cannot contain colon".to_owned());
        }
    }
    let path = match rest.strip_prefix(b"//") {
        Some(after) if has_scheme || !rest.starts_with(b"///") => {
            let (authority, path) = match after.iter().position(|&byte| byte == b'/') {
                Some(slash) => after.split_at(slash),
                None => (after, &b""[..]),
            };
            parse_authority(authority)?;
            path
        }
        _ => rest,
    };
    unescape(path, Mode::Path).map(drop)
}

/// Go's `getScheme`: whether `raw` starts with a scheme, and the text after it and its `:`.
fn scheme(raw: &[u8]) -> Result<(bool, &[u8]), String> {
    for (index, &byte) in raw.iter().enumerate() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' => {}
            b'0'..=b'9' | b'+' | b'-' | b'.' if index > 0 => {}
            b':' if index == 0 => return Err("missing protocol scheme".to_owned()),
            b':' => return Ok((true, raw.get(index + 1..).unwrap_or_default())),
            _ => return Ok((false, raw)),
        }
    }
    Ok((false, raw))
}

/// Go's `parseAuthority`: the host after the last `@`, then the user information before it.
fn parse_authority(authority: &[u8]) -> Result<(), String> {
    let Some(at) = authority.iter().rposition(|&byte| byte == b'@') else {
        return parse_host(authority);
    };
    let (userinfo, host) = authority.split_at(at);
    parse_host(host.get(1..).unwrap_or_default())?;
    if !userinfo.iter().all(|&byte| is_userinfo_byte(byte)) {
        return Err("net/url: invalid userinfo".to_owned());
    }
    let (username, password) = cut(userinfo, b':');
    unescape(username, Mode::UserPassword)?;
    if let Some(password) = password {
        unescape(password, Mode::UserPassword)?;
    }
    Ok(())
}

/// Go's `validUserinfo`, byte by byte: a byte outside ASCII belongs to a rune it rejects.
fn is_userinfo_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._:~!$&'()*+,;=%@".contains(&byte)
}

/// Go's `parseHost` for a scheme other than `http` and `https`: an IP literal in brackets, or a
/// name with an optional port after its last `:`.
fn parse_host(host: &[u8]) -> Result<(), String> {
    match host.iter().rposition(|&byte| byte == b'[') {
        Some(0) => parse_ip_literal(host),
        Some(_) => Err("invalid IP-literal".to_owned()),
        None => {
            if let Some(colon) = host.iter().rposition(|&byte| byte == b':') {
                let port = host.get(colon..).unwrap_or_default();
                if !is_optional_port(port) {
                    return Err(invalid_port(port));
                }
            }
            unescape(host, Mode::Host).map(drop)
        }
    }
}

/// The bracket branch of Go's `parseHost`, as in `[fe80::1%25en0]:80`.
fn parse_ip_literal(host: &[u8]) -> Result<(), String> {
    let Some(close) = host.iter().rposition(|&byte| byte == b']') else {
        return Err("missing ']' in host".to_owned());
    };
    let port = host.get(close + 1..).unwrap_or_default();
    if !is_optional_port(port) {
        return Err(invalid_port(port));
    }
    unescape(port, Mode::Host)?;
    let name = host.get(1..close).unwrap_or_default();
    let unescaped = match find(name, b"%25") {
        Some(zone) => {
            let (address, zone) = name.split_at(zone);
            let mut unescaped = unescape(address, Mode::Host)?;
            unescaped.extend(unescape(zone, Mode::Zone)?);
            unescaped
        }
        None => unescape(name, Mode::Host)?,
    };
    // An address that parses is IPv4, which brackets may not hold.
    match parse_addr(&unescaped) {
        Err(err) => Err(format!("invalid host: {err}")),
        Ok(()) => Err("invalid IP-literal".to_owned()),
    }
}

/// Go's `validOptionalPort`: empty, or `:` followed by digits.
fn is_optional_port(port: &[u8]) -> bool {
    match port.split_first() {
        None => true,
        Some((b':', digits)) => digits.iter().all(u8::is_ascii_digit),
        Some(_) => false,
    }
}

fn invalid_port(port: &[u8]) -> String {
    format!("invalid port {} after host", quote_bytes(port))
}

/// The parts of a URL that Go's `unescape` decodes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Path,
    Host,
    Zone,
    UserPassword,
    Fragment,
}

/// Go's `unescape`: decodes the `%XX` escapes of `s`, or returns the error for a malformed
/// escape, or for a character that a host or zone may not contain.
fn unescape(s: &[u8], mode: Mode) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len());
    let mut index = 0;
    while let Some(&byte) = s.get(index) {
        if byte != b'%' {
            if matches!(mode, Mode::Host | Mode::Zone) && byte < 0x80 && !is_host_byte(byte) {
                return Err(invalid_host(byte));
            }
            out.push(byte);
            index += 1;
            continue;
        }
        let escape = s.get(index..index + 3);
        let Some(&[_, high, low]) = escape.filter(|_| index + 2 < s.len()) else {
            return Err(escape_error(s.get(index..).unwrap_or_default()));
        };
        let (Some(high_value), Some(low_value)) = (hex(high), hex(low)) else {
            return Err(escape_error(s.get(index..).unwrap_or_default()));
        };
        let is_percent = high == b'2' && low == b'5';
        if mode == Mode::Host && high_value < 8 && !is_percent {
            return Err(escape_error(s.get(index..index + 3).unwrap_or_default()));
        }
        let value = (high_value << 4) | low_value;
        if mode == Mode::Zone && !is_percent && value != b' ' && !is_host_byte(value) {
            return Err(escape_error(s.get(index..index + 3).unwrap_or_default()));
        }
        out.push(value);
        index += 3;
    }
    Ok(out)
}

/// Go's `EscapeError` for the text from a `%`, cut to three bytes.
fn escape_error(text: &[u8]) -> String {
    let text = text.get(..3).unwrap_or(text);
    format!("invalid URL escape {}", quote_bytes(text))
}

/// Go's `InvalidHostError`.
fn invalid_host(byte: u8) -> String {
    format!("invalid character {} in host name", quote_bytes(&[byte]))
}

/// Whether Go's `shouldEscape` leaves `byte` unescaped in a host or a zone.
fn is_host_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!$&'()*+,;=:[]<>\"-_.~".contains(&byte)
}

fn hex(byte: u8) -> Option<u8> {
    char::from(byte)
        .to_digit(16)
        .and_then(|digit| u8::try_from(digit).ok())
}

/// Go's `netip.ParseAddr`, which reports errors as `ParseAddr("in"): msg (at "rest")`.
///
/// Go parses `s` as IPv6 if its first `.`, `:`, or `%` is a `:`. A Docker URI cannot get there:
/// its host has no `:`, because the tag starts at the first one, and the host may not escape a
/// `:` as `%3A`. Only the zone after `%25` can hold a `:`, and the `%` comes first.
fn parse_addr(s: &[u8]) -> Result<(), String> {
    for &byte in s {
        match byte {
            b'.' => return parse_ipv4_fields(s, s),
            b'%' => return Err(addr_error(s, "missing IPv6 address", b"")),
            _ => {}
        }
    }
    Err(addr_error(s, "unable to parse IP", b""))
}

/// Go's `parseAddrError.Error`.
fn addr_error(input: &[u8], message: &str, at: &[u8]) -> String {
    let mut text = format!("ParseAddr({}): {message}", quote_bytes(input));
    if !at.is_empty() {
        text.push_str(" (at ");
        text.push_str(&quote_bytes(at));
        text.push(')');
    }
    text
}

/// Go's `parseIPv4Fields` on `s`, a part of `input`, which the errors quote.
fn parse_ipv4_fields(input: &[u8], s: &[u8]) -> Result<(), String> {
    let mut value: u32 = 0;
    let mut position = 0;
    let mut digits = 0;
    for (index, &byte) in s.iter().enumerate() {
        if byte.is_ascii_digit() {
            if digits == 1 && value == 0 {
                return Err(addr_error(
                    input,
                    "IPv4 field has octet with leading zero",
                    b"",
                ));
            }
            value = value * 10 + u32::from(byte - b'0');
            digits += 1;
            if value > 255 {
                return Err(addr_error(input, "IPv4 field has value >255", b""));
            }
        } else if byte == b'.' {
            if index == 0 || index == s.len() - 1 || s.get(index - 1) == Some(&b'.') {
                return Err(addr_error(
                    input,
                    "IPv4 field must have at least one digit",
                    s.get(index..).unwrap_or_default(),
                ));
            }
            if position == 3 {
                return Err(addr_error(input, "IPv4 address too long", b""));
            }
            position += 1;
            value = 0;
            digits = 0;
        } else {
            return Err(addr_error(
                input,
                "unexpected character",
                s.get(index..).unwrap_or_default(),
            ));
        }
    }
    if position < 3 {
        return Err(addr_error(input, "IPv4 address too short", b""));
    }
    Ok(())
}

/// Go's `strings.Cut` on bytes: the text before the first `separator` and, if there is one, the
/// text after it.
fn cut(s: &[u8], separator: u8) -> (&[u8], Option<&[u8]>) {
    match s.iter().position(|&byte| byte == separator) {
        Some(index) => (s.get(..index).unwrap_or_default(), s.get(index + 1..)),
        None => (s, None),
    }
}

/// Returns the index of the first `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests;
