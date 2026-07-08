// @case description Ported old classifier cases: connected remote, nonliteral, and inline script injection
// @tool glass-lint rules=obsidian:dynamic_code

const script = document.createElement("script");
script.src = "https://cdn.example.com/plugin.js";
document.head.appendChild(script);

const nonliteral = document.createElement("script");
nonliteral.src = getPluginUrl();
document.head.append(nonliteral);

const inline = document.createElement("script");
inline.textContent = generatedCode;
document.body.prepend(inline);

const attributed = document.createElement("script");
attributed.setAttribute("src", relativeUrl);
document.documentElement.insertBefore(attributed, document.body);

const appended = document.createElement("script");
appended.append(generatedCode);
document.head.appendChild(appended);
