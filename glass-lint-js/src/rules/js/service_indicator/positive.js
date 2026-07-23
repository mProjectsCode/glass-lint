// @case description positive fixture for js:network.service-indicator
// @tool glass-lint rules=js:network.service-indicator
// @expect-error glass-lint rule=js:network.service-indicator
import "openai/helpers";
// Every configured service SDK module is an exact module-provenance match.
// @expect-error glass-lint rule=js:network.service-indicator
import openai from "openai";
// @expect-error glass-lint rule=js:network.service-indicator
import firebase from "firebase";
// @expect-error glass-lint rule=js:network.service-indicator
import dropbox from "dropbox";
// @expect-error glass-lint rule=js:network.service-indicator
import { createClient } from "@supabase/supabase-js";
// @expect-error glass-lint rule=js:network.service-indicator
import s3 from "@aws-sdk/client-s3";
// Additional exact cloud and service clients remain provider indicators.
// @expect-error glass-lint rule=js:network.service-indicator
import dynamodb from "@aws-sdk/client-dynamodb";
// @expect-error glass-lint rule=js:network.service-indicator
import firestore from "@google-cloud/firestore";
// @expect-error glass-lint rule=js:network.service-indicator
import identity from "@azure/identity";
// @expect-error glass-lint rule=js:network.service-indicator
import storage from "@google-cloud/storage";
// @expect-error glass-lint rule=js:network.service-indicator
import blob from "@azure/storage-blob";
// @expect-error glass-lint rule=js:network.service-indicator
import stripe from "stripe";
// @expect-error glass-lint rule=js:network.service-indicator
import stripeBrowser from "@stripe/stripe-js";
// @expect-error glass-lint rule=js:network.service-indicator
import twilio from "twilio";
// @expect-error glass-lint rule=js:network.service-indicator
import sendgrid from "@sendgrid/mail";
// @expect-error glass-lint rule=js:network.service-indicator
import mailgun from "mailgun.js";
// @expect-error glass-lint rule=js:network.service-indicator
import octokit from "@octokit/rest";

// @expect-error glass-lint rule=js:network.service-indicator
const awsEndpoint = "https://s3.amazonaws.com/bucket";
// @expect-error glass-lint rule=js:network.service-indicator
const stripeEndpoint = "https://api.stripe.com/v1";
// @expect-error glass-lint rule=js:network.service-indicator
const twilioEndpoint = "https://api.twilio.com/2010";
// @expect-error glass-lint rule=js:network.service-indicator
const sendgridEndpoint = "https://api.sendgrid.com/v3/mail/send";
// @expect-error glass-lint rule=js:network.service-indicator
const slackEndpoint = "https://slack.com/api/chat.postMessage";
