// @case description negative fixture for js:network.service-indicator
// @tool glass-lint rules=js:network.service-indicator
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
import "openai-extra";
// Similar module names do not establish provenance.
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
import unrelatedOpenAI from "openai-client";
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
import localService from "@supabase/supabase";
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
import unrelatedSdk from "@aws-sdk/client-dynamodb-helper";

// Unconfigured domains and ordinary prose are ignored.
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
const ordinaryDomain = "example.net";
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
const unrelatedEndpoint = "https://api.sendgrid.example";
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
const unrelatedProvider = "mastodon posthog headers";

// Literal matching does not reconstruct concatenated or dynamic values.
const concatenated = "https://api." + "openai.com";
const host = getHost();
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
const dynamicEndpoint = "https://" + host;

// A local helper is unrelated to the marker matchers.
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
function localLookalike() { return null; }
localLookalike();
