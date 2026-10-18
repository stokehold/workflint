//! The errors of Go's `url.Parse` for Docker URIs.

use super::parse_error;

/// The image after `docker://`, and the error text that Go 1.26.4's `url.Parse` returns for the
/// URI, written by a Go program from these images.
const GO_CASES: [(&str, Option<&str>); 48] = [
    ("alpine", None),
    ("ghcr.io/owner/image", None),
    ("", None),
    (
        "image with space",
        Some("parse \"docker://image with space\": invalid character \" \" in host name"),
    ),
    (
        "a\x01b",
        Some("parse \"docker://a\\x01b\": net/url: invalid control character in URL"),
    ),
    (
        "a\x7fb",
        Some("parse \"docker://a\\x7fb\": net/url: invalid control character in URL"),
    ),
    (
        "host/p%zz",
        Some("parse \"docker://host/p%zz\": invalid URL escape \"%zz\""),
    ),
    (
        "host/p%4",
        Some("parse \"docker://host/p%4\": invalid URL escape \"%4\""),
    ),
    (
        "host/p%",
        Some("parse \"docker://host/p%\": invalid URL escape \"%\""),
    ),
    ("host/%e2%82", None),
    (
        "ho%41st",
        Some("parse \"docker://ho%41st\": invalid URL escape \"%41\""),
    ),
    ("ho%c3%a9st", None),
    (
        "ho%zzst",
        Some("parse \"docker://ho%zzst\": invalid URL escape \"%zz\""),
    ),
    (
        "h{o}st",
        Some("parse \"docker://h{o}st\": invalid character \"{\" in host name"),
    ),
    (
        "h|ost",
        Some("parse \"docker://h|ost\": invalid character \"|\" in host name"),
    ),
    ("user@host", None),
    (
        "us er@host",
        Some("parse \"docker://us er@host\": net/url: invalid userinfo"),
    ),
    (
        "us%zz@host",
        Some("parse \"docker://us%zz@host\": invalid URL escape \"%zz\""),
    ),
    ("a@b@host", None),
    ("host#frag", None),
    (
        "host#fr%zz",
        Some("parse \"docker://host#fr%zz\": invalid URL escape \"%zz\""),
    ),
    ("host?q%zz", None),
    ("host/x?", None),
    ("[", Some("parse \"docker://[\": missing ']' in host")),
    ("a[", Some("parse \"docker://a[\": invalid IP-literal")),
    ("a[b]", Some("parse \"docker://a[b]\": invalid IP-literal")),
    (
        "[abc]",
        Some("parse \"docker://[abc]\": invalid host: ParseAddr(\"abc\"): unable to parse IP"),
    ),
    (
        "[abc]x",
        Some("parse \"docker://[abc]x\": invalid port \"x\" after host"),
    ),
    (
        "[1.2.3.4]",
        Some("parse \"docker://[1.2.3.4]\": invalid IP-literal"),
    ),
    (
        "[1.2.3]",
        Some(
            "parse \"docker://[1.2.3]\": invalid host: ParseAddr(\"1.2.3\"): IPv4 address too short",
        ),
    ),
    (
        "[01.2.3.4]",
        Some(
            "parse \"docker://[01.2.3.4]\": invalid host: ParseAddr(\"01.2.3.4\"): IPv4 field has octet with leading zero",
        ),
    ),
    (
        "[1.2.3.256]",
        Some(
            "parse \"docker://[1.2.3.256]\": invalid host: ParseAddr(\"1.2.3.256\"): IPv4 field has value >255",
        ),
    ),
    (
        "[1..2.3]",
        Some(
            "parse \"docker://[1..2.3]\": invalid host: ParseAddr(\"1..2.3\"): IPv4 field must have at least one digit (at \".2.3\")",
        ),
    ),
    (
        "[1.2.3.]",
        Some(
            "parse \"docker://[1.2.3.]\": invalid host: ParseAddr(\"1.2.3.\"): IPv4 field must have at least one digit (at \".\")",
        ),
    ),
    (
        "[1.2.3.4.5]",
        Some(
            "parse \"docker://[1.2.3.4.5]\": invalid host: ParseAddr(\"1.2.3.4.5\"): IPv4 address too long",
        ),
    ),
    (
        "[1.2.3.x]",
        Some(
            "parse \"docker://[1.2.3.x]\": invalid host: ParseAddr(\"1.2.3.x\"): unexpected character (at \"x\")",
        ),
    ),
    (
        "[%25]",
        Some("parse \"docker://[%25]\": invalid host: ParseAddr(\"%\"): missing IPv6 address"),
    ),
    (
        "[%25eth0]",
        Some(
            "parse \"docker://[%25eth0]\": invalid host: ParseAddr(\"%eth0\"): missing IPv6 address",
        ),
    ),
    (
        "[%25%0A]",
        Some("parse \"docker://[%25%0A]\": invalid URL escape \"%0A\""),
    ),
    (
        "[%25%20]",
        Some("parse \"docker://[%25%20]\": invalid host: ParseAddr(\"% \"): missing IPv6 address"),
    ),
    (
        "[%25x%e2%82]",
        Some("parse \"docker://[%25x%e2%82]\": invalid URL escape \"%e2\""),
    ),
    (
        "[%c3%a9.1]",
        Some(
            "parse \"docker://[%c3%a9.1]\": invalid host: ParseAddr(\"é.1\"): unexpected character (at \"é.1\")",
        ),
    ),
    (
        "[1.2.3.4%25eth0]",
        Some(
            "parse \"docker://[1.2.3.4%25eth0]\": invalid host: ParseAddr(\"1.2.3.4%eth0\"): unexpected character (at \"%eth0\")",
        ),
    ),
    (
        "[1%25%3A%3A1]",
        Some(
            "parse \"docker://[1%25%3A%3A1]\": invalid host: ParseAddr(\"1%::1\"): missing IPv6 address",
        ),
    ),
    (
        "[%e2%82.1]",
        Some(
            "parse \"docker://[%e2%82.1]\": invalid host: ParseAddr(\"\\xe2\\x82.1\"): unexpected character (at \"\\xe2\\x82.1\")",
        ),
    ),
    (
        "host/%e2%82%",
        Some("parse \"docker://host/%e2%82%\": invalid URL escape \"%\""),
    ),
    (
        "#%zz",
        Some("parse \"docker://#%zz\": invalid URL escape \"%zz\""),
    ),
    (
        "host?#%e2%8",
        Some("parse \"docker://host?#%e2%8\": invalid URL escape \"%8\""),
    ),
];

#[test]
fn errors_match_go_url_parse() {
    for (image, expected) in GO_CASES {
        let uri = format!("docker://{image}");
        assert_eq!(parse_error(&uri).as_deref(), expected, "{uri}");
    }
}
