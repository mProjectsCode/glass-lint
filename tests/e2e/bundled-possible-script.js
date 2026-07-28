// @case description Bundled control-flow retains a strict possible-path witness
// @case tags bundled,certainty,evidence
// @tool glass-lint rules=browser:dynamic-code.script-injection
// @expect-error glass-lint rule=browser:dynamic-code.script-injection certainty=possible
const e=document.createElement("script");if(window.__load)e.src="https://cdn.example.test/a.js";document.head.appendChild(e);
