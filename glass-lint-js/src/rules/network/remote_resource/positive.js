// @case description positive fixture for js:dom.remote-resource
// @tool glass-lint rules=js:dom.remote-resource
// @expect-error glass-lint rule=js:dom.remote-resource message_id=detected
const script = document.createElement("script");
script.src = "https://example.com/plugin.js";
document.head.appendChild(script);
// second independent example
// @expect-error glass-lint rule=js:dom.remote-resource message_id=detected
const secondScript = document.createElement("script"); secondScript.src = "https://example.com/second.js"; document.head.appendChild(secondScript);
