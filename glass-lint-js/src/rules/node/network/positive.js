// @case description positive fixture for js:node.network
// @tool glass-lint rules=js:node.network
// @expect-error glass-lint rule=js:node.network message_id=detected
import http from "node:http";
// second independent example
// @expect-error glass-lint rule=js:node.network message_id=detected
import * as secondHttp from "node:http";
