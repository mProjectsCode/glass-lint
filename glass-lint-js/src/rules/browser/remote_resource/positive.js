// @case description positive fixture for browser:dom.remote-resource
// @tool glass-lint rules=browser:dom.remote-resource
const script = document.createElement("script");
script.src = "https://example.com/plugin.js";
document.head.appendChild(script);
// @expect-error-after glass-lint rule=browser:dom.remote-resource
// An alias and setAttribute configuration retain the tracked element flow.
const image = document.createElement("img");
const remoteImage = image;
remoteImage.setAttribute("src", "//cdn.example.com/logo.png");
document.body.append(remoteImage);
// @expect-error-after glass-lint rule=browser:dom.remote-resource
const stylesheet = document.createElement("link");
stylesheet.href = "https://cdn.example.com/theme.css";
document.head.appendChild(stylesheet);
// @expect-error-after glass-lint rule=browser:dom.remote-resource
const frame = document.createElement("iframe");
frame.src = "//example.com/frame.html";
document.body.appendChild(frame);
// @expect-error-after glass-lint rule=browser:dom.remote-resource
const media = document.createElement("video");
media.src = "https://media.example.com/video.mp4";
document.body.appendChild(media);
// @expect-error-after glass-lint rule=browser:dom.remote-resource
