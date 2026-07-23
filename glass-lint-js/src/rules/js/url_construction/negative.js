// @case description negative fixture for js:network.url-construction
// @tool glass-lint rules=js:network.url-construction
// A local shadow is not the browser global constructor.
function localConstructors(URL, URLSearchParams) {
    // @expect-no-error glass-lint rule=js:network.url-construction
    new URL("local");
    // @expect-no-error glass-lint rule=js:network.url-construction
    new URLSearchParams("local=1");
    // @expect-no-error glass-lint rule=js:network.url-construction
    URL.parse("local");
    // @expect-no-error glass-lint rule=js:network.url-construction
    URL.canParse("local");
}
localConstructors(() => {}, () => {});

// Reassignment drops provenance from a global constructor alias.
function LocalURL() {}
let reassignedURL = URL;
reassignedURL = LocalURL;
// @expect-no-error glass-lint rule=js:network.url-construction
new reassignedURL("local");

// Other URL-like names and plain strings are outside this rule.
function URLPattern() {}
// @expect-no-error glass-lint rule=js:network.url-construction
new URLPattern();
// URL-like prose without a scheme delimiter is not a literal URL marker.
// @expect-no-error glass-lint rule=js:network.url-construction
const urlText = "https endpoint";
// @expect-no-error glass-lint rule=js:network.url-construction
URL.createObjectURL = localCreateObjectURL;
// @expect-no-error glass-lint rule=js:network.url-construction
URL.createObjectURL(blob);
