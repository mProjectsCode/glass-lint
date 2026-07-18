// @case description positive fixture for js:network.service-indicator
// @tool glass-lint rules=js:network.service-indicator
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import "openai/helpers";
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
import s3 from "@aws-sdk/client-s3";
// Additional exact cloud and service clients remain provider indicators.
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import dynamodb from "@aws-sdk/client-dynamodb";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import firestore from "@google-cloud/firestore";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import identity from "@azure/identity";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import storage from "@google-cloud/storage";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import blob from "@azure/storage-blob";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import stripe from "stripe";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import stripeBrowser from "@stripe/stripe-js";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import twilio from "twilio";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import sendgrid from "@sendgrid/mail";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import mailgun from "mailgun.js";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import octokit from "@octokit/rest";

// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const awsEndpoint = "https://s3.amazonaws.com/bucket";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const stripeEndpoint = "https://api.stripe.com/v1";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const twilioEndpoint = "https://api.twilio.com/2010";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const sendgridEndpoint = "https://api.sendgrid.com/v3/mail/send";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const slackEndpoint = "https://slack.com/api/chat.postMessage";
