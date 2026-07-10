// @case description DOM insertion flow rejects disconnected, local, future, and reassigned elements
// @tool glass-lint rules=obsidian:dynamic_code,obsidian:network.remote_dom_loading
// @tool eslint-obsidianmd config=recommended

const configuredOnly = document.createElement("script");
configuredOnly.src = "https://cdn.example.com/plugin.js";

const futureScript = document.createElement("script");
document.head.appendChild(futureScript);
futureScript.src = "https://cdn.example.com/plugin.js";

let reassignedScript = document.createElement("script");
reassignedScript.src = "https://cdn.example.com/plugin.js";
reassignedScript = document.createElement("div");
document.head.appendChild(reassignedScript);

const localImage = document.createElement("img");
localImage.src = "/logo.png";
document.body.appendChild(localImage);

const dynamicImage = document.createElement("img");
dynamicImage.src = getLogoUrl();
document.body.appendChild(dynamicImage);

const linkWithoutRel = document.createElement("link");
linkWithoutRel.href = "https://cdn.example.com/theme.css";
document.head.appendChild(linkWithoutRel);

const localStyle = document.createElement("style");
localStyle.textContent = ".icon { background: url('/local.png') }";
document.head.appendChild(localStyle);
