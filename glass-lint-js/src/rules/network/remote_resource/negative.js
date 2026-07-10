// @case description negative fixture for js:dom.remote-resource
// @tool glass-lint rules=js:dom.remote-resource
// @expect-no-error glass-lint rule=js:dom.remote-resource message_id=detected
function localLookalike() { return null; }
localLookalike();
const localScript = document.createElement("script");
// @expect-no-error glass-lint rule=js:dom.remote-resource message_id=detected
localScript.src = "/local.js";

// Migrated: system/dom-insertion-negative-flow.js
const legacyConfiguredOnly = document.createElement("script");
legacyConfiguredOnly.src = "https://cdn.example.com/plugin.js";
const legacyLocalImage = document.createElement("img");
legacyLocalImage.src = "/logo.png";
document.body.appendChild(legacyLocalImage);
const legacyLinkWithoutRel = document.createElement("link");
legacyLinkWithoutRel.href = "https://cdn.example.com/theme.css";
document.head.appendChild(legacyLinkWithoutRel);
