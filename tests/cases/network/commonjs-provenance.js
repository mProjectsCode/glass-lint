// @case description CommonJS Obsidian imports preserve module provenance
// @tool glass-lint rules=obsidian:network.request
// @tool eslint-obsidianmd config=recommended

var obsidian = require("obsidian");
obsidian.requestUrl("https://example.com"); // @expect-error glass-lint rule=obsidian:network.request message_id=detected count=8 line=any
(0, obsidian["requestUrl"])("https://example.com");

var { requestUrl: r } = require("obsidian");
r("https://example.com");

var wrapped = __toESM(require("obsidian"));
wrapped.requestUrl("https://example.com");

const wrappedStar = __importStar(require("obsidian"));
wrappedStar.requestUrl("https://example.com");

const wrappedDefault = __importDefault(require("obsidian"));
wrappedDefault.requestUrl("https://example.com");

const wildcard = _interopRequireWildcard(require("obsidian"));
wildcard.requestUrl("https://example.com");

const defaultInterop = _interopRequireDefault(require("obsidian"));
defaultInterop.requestUrl("https://example.com");
