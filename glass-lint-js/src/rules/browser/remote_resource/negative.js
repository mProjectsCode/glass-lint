// @case description negative fixture for browser:dom.remote-resource
// @tool glass-lint rules=browser:dom.remote-resource
// @expect-no-error glass-lint rule=browser:dom.remote-resource
// Local URLs are not remote resources.
const localScript = document.createElement("script");
localScript.src = "/local.js";
document.body.appendChild(localScript);

// Dynamic values, unsupported element tags, and no sink are excluded.
const dynamicScript = document.createElement("script");
dynamicScript.src = remoteUrl;
document.head.appendChild(dynamicScript);
const link = document.createElement("link");
link.src = "https://cdn.example.com/theme.css";
document.head.appendChild(link);
const configuredOnly = document.createElement("img");
configuredOnly.src = "https://cdn.example.com/logo.png";
