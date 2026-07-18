// @case description positive fixture for node:node.network
// @tool glass-lint rules=node:node.network
// Every configured HTTP module is reported at its ESM load.
// @expect-error glass-lint rule=node:node.network message_id=detected
import http from "http";
// @expect-error glass-lint rule=node:node.network message_id=detected
import https from "https";
// @expect-error glass-lint rule=node:node.network message_id=detected
import nodeHttp from "node:http";
// @expect-error glass-lint rule=node:node.network message_id=detected
import nodeHttps from "node:https";

// Unshadowed static CommonJS loads retain module provenance.
// @expect-error glass-lint rule=node:node.network message_id=detected
const loadedHttp = require("http");
// @expect-error glass-lint rule=node:node.network message_id=detected
const loadedHttps = require("node:https");
// @expect-error glass-lint rule=node:node.network message_id=detected
import http2 from "node:http2";
// @expect-error glass-lint rule=node:node.network message_id=detected
import net from "node:net";
// @expect-error glass-lint rule=node:node.network message_id=detected
import tls from "node:tls";
// @expect-error glass-lint rule=node:node.network message_id=detected
import dgram from "node:dgram";
// @expect-error glass-lint rule=node:node.network message_id=detected
import dns from "node:dns";
// @expect-error glass-lint rule=node:node.network message_id=detected
import dnsPromises from "dns/promises";
// @expect-error glass-lint rule=node:node.network message_id=detected
import nodeDnsPromises from "node:dns/promises";
// @expect-error glass-lint rule=node:node.network message_id=detected
import undici from "undici";
// @expect-error glass-lint rule=node:node.network message_id=detected
import axios from "axios";
// @expect-error glass-lint rule=node:node.network message_id=detected
import fetchClient from "node-fetch";
// @expect-error glass-lint rule=node:node.network message_id=detected
import got from "got";
// @expect-error glass-lint rule=node:node.network message_id=detected
import superagent from "superagent";
// @expect-error glass-lint rule=node:node.network message_id=detected
import ws from "ws";
// @expect-error glass-lint rule=node:node.network message_id=detected
import crossFetch from "cross-fetch";
// @expect-error glass-lint rule=node:node.network message_id=detected
import ky from "ky";
// @expect-error glass-lint rule=node:node.network message_id=detected
import graphqlRequest from "graphql-request";
// @expect-error glass-lint rule=node:node.network message_id=detected
import request from "request";
// @expect-error glass-lint rule=node:node.network message_id=detected
import needle from "needle";
// @expect-error glass-lint rule=node:node.network message_id=detected
import grpc from "@grpc/grpc-js";
// Additional exact clients and transport helpers remain network indicators.
// @expect-error glass-lint rule=node:node.network message_id=detected
import apollo from "@apollo/client";
// @expect-error glass-lint rule=node:node.network message_id=detected
import graphql from "graphql";
// @expect-error glass-lint rule=node:node.network message_id=detected
import elastic from "@elastic/elasticsearch";
// @expect-error glass-lint rule=node:node.network message_id=detected
import retry from "fetch-retry";
// @expect-error glass-lint rule=node:node.network message_id=detected
import formData from "form-data";
// @expect-error glass-lint rule=node:node.network message_id=detected
import proxy from "http-proxy";
// @expect-error glass-lint rule=node:node.network message_id=detected
import proxyAgent from "https-proxy-agent";
// @expect-error glass-lint rule=node:node.network message_id=detected
import fetchImpl from "@whatwg-node/fetch";
