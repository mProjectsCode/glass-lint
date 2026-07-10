// @case description positive fixture for js:node.network
// @tool glass-lint rules=js:node.network
// Every configured HTTP module is reported at its ESM load.
// @expect-error glass-lint rule=js:node.network message_id=detected
import http from "http";
// @expect-error glass-lint rule=js:node.network message_id=detected
import https from "https";
// @expect-error glass-lint rule=js:node.network message_id=detected
import nodeHttp from "node:http";
// @expect-error glass-lint rule=js:node.network message_id=detected
import nodeHttps from "node:https";

// Unshadowed static CommonJS loads retain module provenance.
// @expect-error glass-lint rule=js:node.network message_id=detected
const loadedHttp = require("http");
// @expect-error glass-lint rule=js:node.network message_id=detected
const loadedHttps = require("node:https");
