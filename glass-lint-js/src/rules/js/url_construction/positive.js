// @case description positive fixture for js:network.url-construction
// @tool glass-lint rules=js:network.url-construction
// Direct construction is detected for both configured globals.
const target = getUrl();
// @expect-error glass-lint rule=js:network.url-construction
new URL(target);
// @expect-error glass-lint rule=js:network.url-construction
new URLSearchParams("a=1");
// @expect-error glass-lint rule=js:network.url-construction
URL.parse(target);
// @expect-error glass-lint rule=js:network.url-construction
URL.canParse(target);
// Static URL object-URL helpers are also URL construction/access points.
// @expect-error glass-lint rule=js:network.url-construction
URL.createObjectURL(blob);
// @expect-error glass-lint rule=js:network.url-construction
URL.revokeObjectURL(objectUrl);
// Static HTTP(S) literals are URL references even without a constructor call.
// @expect-error glass-lint rule=js:network.url-construction
const literalUrl = "https://literal.example/resource";
// @expect-error glass-lint rule=js:network.url-construction
const httpLiteral = "http://legacy.example/path";

// Constructor detection is independent of static or dynamic arguments.
// @expect-error glass-lint rule=js:network.url-construction
new URL(target);

// Direct aliases retain global constructor provenance.
const URLAlias = URL;
// @expect-error glass-lint rule=js:network.url-construction
new URLAlias("/aliased");
const ParamsAlias = URLSearchParams;
// @expect-error glass-lint rule=js:network.url-construction
new ParamsAlias(target);
