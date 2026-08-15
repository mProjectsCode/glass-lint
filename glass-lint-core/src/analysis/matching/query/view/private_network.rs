use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

pub(in crate::analysis::matching) fn private_network_match(value: &str) -> Option<(usize, usize)> {
    contains_localhost(value)
        .or_else(|| contains_private_ipv4(value))
        .or_else(|| contains_private_ipv6(value))
}

fn contains_localhost(value: &str) -> Option<(usize, usize)> {
    let lowered = value.to_ascii_lowercase();
    let bytes = lowered.as_bytes();
    lowered.match_indices("localhost").find_map(|(index, _)| {
        let before = index.checked_sub(1).and_then(|i| bytes.get(i));
        let after = bytes.get(index + "localhost".len());
        (before.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'.')
            && after.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'.'))
        .then_some((index, index + "localhost".len()))
    })
}

fn contains_private_ipv4(value: &str) -> Option<(usize, usize)> {
    let bytes = value.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit()
            || (start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'.'))
        {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
            end += 1;
        }
        let candidate = &value[start..end];
        let before_is_boundary = start == 0
            || (!bytes[start - 1].is_ascii_alphanumeric()
                && bytes[start - 1] != b'.'
                && bytes[start - 1] != b'\\');
        let boundary =
            end == bytes.len() || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'.');
        if candidate.matches('.').count() == 3
            && before_is_boundary
            && boundary
            && IpAddr::from_str(candidate).is_ok_and(|ip| match ip {
                IpAddr::V4(ip) => private_ipv4(ip),
                IpAddr::V6(ip) => private_ipv6(ip),
            })
        {
            return Some((start, end));
        }
        start = end.max(start + 1);
    }
    None
}

fn contains_private_ipv6(value: &str) -> Option<(usize, usize)> {
    let mut token_start = 0;
    for (index, character) in value.char_indices() {
        if character.is_whitespace()
            || matches!(character, '"' | '\'' | '(' | ')' | ',' | '=' | '?' | '#')
        {
            if let Some(found) = private_ipv6_token(value, token_start, index) {
                return Some(found);
            }
            token_start = index + character.len_utf8();
        }
    }
    private_ipv6_token(value, token_start, value.len())
}

fn private_ipv6_token(value: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let token = &value[start..end];
    let (token, token_start) = token.strip_prefix("http://").map_or_else(
        || {
            token
                .strip_prefix("https://")
                .map_or((token, start), |host| (host, start + "https://".len()))
        },
        |host| (host, start + "http://".len()),
    );
    let slash = token.find('/').unwrap_or(token.len());
    let token = &token[..slash];
    let (host, host_start) = token
        .strip_prefix('[')
        .map_or((token, token_start), |host| (host, token_start + 1));
    let host_end = host.find(']').unwrap_or(host.len());
    let host = &host[..host_end];
    let before = value[..host_start].chars().next_back();
    let after = value[host_start + host.len()..].chars().next();
    if before.is_some_and(|character| matches!(character, '?' | '\\'))
        || after.is_some_and(|character| matches!(character, '?' | '\\'))
    {
        return None;
    }
    host.contains(':')
        .then(|| Ipv6Addr::from_str(host).ok())
        .flatten()
        .filter(|ip| private_ipv6(*ip))
        .map(|_| (host_start, host_start + host.len()))
}

fn private_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, d] = ip.octets();
    (a == 0 && b == 0 && c == 0 && d == 0)
        || a == 10
        || a == 127
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..=19).contains(&b))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
}

fn private_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || ip == Ipv6Addr::LOCALHOST
        || ip.is_unspecified()
        || ip.to_ipv4().is_some_and(private_ipv4)
}
