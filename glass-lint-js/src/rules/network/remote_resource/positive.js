// @case description positive fixture for js:dom.remote-resource
// @tool glass-lint rules=js:dom.remote-resource
// @expect-error glass-lint rule=js:dom.remote-resource message_id=detected
const script = document.createElement("script");
script.src = "https://example.com/plugin.js";
document.head.appendChild(script);
// second independent example

// @expect-error glass-lint rule=js:dom.remote-resource message_id=detected
const secondScript = document.createElement("script"); secondScript.src = "https://example.com/second.js"; document.head.appendChild(secondScript);

// @expect-error glass-lint rule=js:dom.remote-resource message_id=detected
const image = document.createElement("img");

// @expect-no-error glass-lint rule=js:dom.remote-resource message_id=detected
image.setAttribute("src", "https://example.com/image.png"); document.body.append(image);
// Migrated: system/static-risk-apis.js

// @expect-error glass-lint rule=js:dom.remote-resource message_id=detected
const remoteImage = document.createElement("img");
remoteImage.src = "https://cdn.example.com/logo.png";
document.body.appendChild(remoteImage);
