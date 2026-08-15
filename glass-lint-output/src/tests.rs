use super::visible_text;

#[test]
fn visible_text_escapes_terminal_controls() {
    assert_eq!(visible_text("a\n\t\u{0001}"), "a\\n\\t\\u{0001}");
}
