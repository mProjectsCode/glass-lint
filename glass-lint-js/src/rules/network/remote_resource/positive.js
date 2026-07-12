// @case description positive fixture for js:dom.remote-resource
// @tool glass-lint rules=js:dom.remote-resource
const script = document.createElement("script");
script.src = "https://example.com/plugin.js";
document.head.appendChild(script);
// @expect-error-after glass-lint rule=js:dom.remote-resource message_id=detected
// An alias and setAttribute configuration retain the tracked element flow.
const image = document.createElement("img");
const remoteImage = image;
remoteImage.setAttribute("src", "//cdn.example.com/logo.png");
document.body.append(remoteImage);
// @expect-error-after glass-lint rule=js:dom.remote-resource message_id=detected
