use crate::analysis::matching::private_network_match;

#[test]
fn regex_ipv6_syntax_is_not_an_address() {
    assert_eq!(
        private_network_match(r"^\s*(?:\(?(?:GMT|UTC)\s?)?([+-])(\d{1,2})(?::?(\d{2}))?\)?"),
        None
    );
    assert_eq!(
        private_network_match(
            r"([0-9]{4})\-([0-9]{1,2})\-([0-9]{1,2})(?:T([0-9]{1,2}):([0-9]{1,2})(?::([0-9]{1,2}))?)?"
        ),
        None
    );
}

#[test]
fn returns_the_address_span() {
    assert_eq!(
        private_network_match("https://192.168.1.2:8080"),
        Some((8, 19))
    );
    assert_eq!(private_network_match("http://[::1]/"), Some((8, 11)));
}
