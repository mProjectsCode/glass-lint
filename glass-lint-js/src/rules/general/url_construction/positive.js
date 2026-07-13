// @case description positive fixture for js:network.url-construction
// @tool glass-lint rules=js:network.url-construction
// Direct construction is detected for both configured globals.
// @expect-error glass-lint rule=js:network.url-construction message_id=detected
new URL("https://example.com");
// @expect-error glass-lint rule=js:network.url-construction message_id=detected
new URLSearchParams("a=1");

// Constructor detection is independent of static or dynamic arguments.
const target = getUrl();
// @expect-error glass-lint rule=js:network.url-construction message_id=detected
new URL(target);

// Direct aliases retain global constructor provenance.
const URLAlias = URL;
// @expect-error glass-lint rule=js:network.url-construction message_id=detected
new URLAlias("/aliased");
const ParamsAlias = URLSearchParams;
// @expect-error glass-lint rule=js:network.url-construction message_id=detected
new ParamsAlias(target);
