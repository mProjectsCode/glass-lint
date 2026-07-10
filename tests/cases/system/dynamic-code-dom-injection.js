// @case description Connected remote, nonliteral, and inline script injection is detected
// @tool glass-lint rules=js:dynamic-code.script-injection
// @tool eslint-obsidianmd config=recommended

const script = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
script.src = "https://cdn.example.com/plugin.js";
document.head.appendChild(script);

const nonliteral = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
nonliteral.src = getPluginUrl();
document.head.append(nonliteral);

const inline = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
inline.textContent = generatedCode;
document.body.prepend(inline);

const attributed = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
attributed.setAttribute("src", relativeUrl);
document.documentElement.insertBefore(attributed, document.body);

const appended = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
appended.append(generatedCode);
document.head.appendChild(appended);
