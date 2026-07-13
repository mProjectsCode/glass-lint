// @case description positive fixture for js:network.service-indicator
// @tool glass-lint rules=js:network.service-indicator
// Every configured service SDK module is an exact module-provenance match.
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import openai from "openai";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import firebase from "firebase";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import dropbox from "dropbox";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import { createClient } from "@supabase/supabase-js";

// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const awsEndpoint = "https://s3.amazonaws.com/bucket";
