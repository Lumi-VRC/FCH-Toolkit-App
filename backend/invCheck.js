const express = require('express');
const axios = require('axios');
const fs = require('fs');
const vrchat = require('vrchat');
const twofactor = require('node-2fa');
const mysql = require('mysql2/promise');

const app = express();
app.use(express.json());

const CONFIG_PATH = './config/impulse.json';
const API_BASE = 'https://api.vrchat.cloud/api/1';
const USER_AGENT = 'FCH-Toolkit/invCheck';
const JOB_DELAY_MS = Number(process.env.INV_CHECK_JOB_DELAY_MS || 4000);
const CACHE_HIT_DELAY_MS = Number(process.env.INV_CHECK_CACHE_HIT_DELAY_MS || 100);
const CACHE_TABLE = 'inv_check_cache';
const DB_CONFIG = {
  host: process.env.INV_CHECK_DB_HOST || process.env.API_CHECKS_DB_HOST || 'localhost',
  user: process.env.INV_CHECK_DB_USER || process.env.API_CHECKS_DB_USER || 'FCHUser',
  password: process.env.INV_CHECK_DB_PASSWORD || process.env.API_CHECKS_DB_PASSWORD || 'Hakukob1!',
  database: process.env.INV_CHECK_DB_NAME || process.env.API_CHECKS_DB_NAME || 'fch_toolkit',
  waitForConnections: true,
  connectionLimit: Number(process.env.INV_CHECK_DB_CONN_LIMIT || process.env.API_CHECKS_DB_CONN_LIMIT || 5),
  queueLimit: 0
};

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

let cfg = null;
let authCookie = null;
let dbPool = null;
let cacheTableEnsured = false;
let cacheTableEnsuring = null;
const jobQueue = [];
let processing = false;
const inflightJobs = new Map();
let shuttingDown = false;

function loadConfig() {
  if (!cfg) {
    cfg = JSON.parse(fs.readFileSync(CONFIG_PATH, 'utf8'));
  }
  return cfg;
}

function saveConfig(nextCfg) {
  fs.writeFileSync(CONFIG_PATH, JSON.stringify(nextCfg, null, 2));
}

async function getDbPool() {
  if (!dbPool) {
    dbPool = mysql.createPool(DB_CONFIG);
  }
  return dbPool;
}

async function ensureCacheTable() {
  if (cacheTableEnsured) return;
  if (cacheTableEnsuring) {
    await cacheTableEnsuring;
    return;
  }
  const pool = await getDbPool();
  const sql = `
    CREATE TABLE IF NOT EXISTS \`${CACHE_TABLE}\` (
      cache_key VARCHAR(128) NOT NULL,
      item_type VARCHAR(32) DEFAULT NULL,
      owner_id VARCHAR(128) DEFAULT NULL,
      image_url TEXT DEFAULT NULL,
      payload_json LONGTEXT NOT NULL,
      updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
      PRIMARY KEY (cache_key)
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
  `;
  cacheTableEnsuring = pool
    .query(sql)
    .then(() => {
      cacheTableEnsured = true;
    })
    .catch((err) => {
      console.error('[invCheck] Failed to ensure cache table:', err?.message || err);
      throw err;
    })
    .finally(() => {
      cacheTableEnsuring = null;
    });
  await cacheTableEnsuring;
}

async function readCachedResult(cacheKey) {
  try {
    const pool = await getDbPool();
    await ensureCacheTable();
    const [rows] = await pool.query(
      `SELECT payload_json FROM \`${CACHE_TABLE}\` WHERE cache_key = ? LIMIT 1`,
      [cacheKey]
    );
    if (rows && rows.length > 0 && rows[0].payload_json) {
      try {
        return JSON.parse(rows[0].payload_json);
      } catch (err) {
        console.warn('[invCheck] Failed to parse cached payload for', cacheKey, err?.message || err);
      }
    }
  } catch (err) {
    console.error('[invCheck] Cache read failed:', err?.message || err);
  }
  return null;
}

async function writeCachedResult(cacheKey, result) {
  try {
    const pool = await getDbPool();
    await ensureCacheTable();
    const payloadJson = JSON.stringify(result);
    const itemType = result?.type ?? null;
    const ownerId = result?.payload?.ownerId ?? null;
    const imageUrl = result?.payload?.imageUrl ?? null;
    await pool.query(
      `INSERT INTO \`${CACHE_TABLE}\` (cache_key, item_type, owner_id, image_url, payload_json)
       VALUES (?, ?, ?, ?, ?)
       ON DUPLICATE KEY UPDATE item_type = VALUES(item_type), owner_id = VALUES(owner_id), image_url = VALUES(image_url), payload_json = VALUES(payload_json), updated_at = CURRENT_TIMESTAMP`,
      [cacheKey, itemType, ownerId, imageUrl, payloadJson]
    );
  } catch (err) {
    console.error('[invCheck] Cache write failed:', err?.message || err);
  }
}

function scheduleJob(cacheKey, task) {
  if (inflightJobs.has(cacheKey)) {
    return inflightJobs.get(cacheKey);
  }

  const jobPromise = new Promise((resolve, reject) => {
    jobQueue.push({ cacheKey, task, resolve, reject });
    void processQueue();
  }).finally(() => {
    inflightJobs.delete(cacheKey);
  });

  inflightJobs.set(cacheKey, jobPromise);
  return jobPromise;
}

async function processQueue() {
  if (processing) return;
  processing = true;
  try {
    while (jobQueue.length > 0 && !shuttingDown) {
      const job = jobQueue.shift();
      if (!job) continue;
      try {
        const result = await job.task();
        job.resolve(result);
        if (JOB_DELAY_MS > 0 && jobQueue.length > 0) {
          await delay(JOB_DELAY_MS);
        }
      } catch (err) {
        job.reject(err);
        // Small delay after failure to avoid hammering the API
        await delay(Math.min(JOB_DELAY_MS, 500));
      }
    }
  } finally {
    processing = false;
    if (jobQueue.length > 0 && !shuttingDown) {
      void processQueue();
    }
  }
}

process.on('SIGINT', () => {
  shuttingDown = true;
});

process.on('SIGTERM', () => {
  shuttingDown = true;
});

function generateOtpToken(secret) {
  const tokenObj = twofactor.generateToken(secret);
  if (!tokenObj || !tokenObj.token) throw new Error('Failed to generate OTP token');
  return tokenObj.token;
}

async function loginAndGetAuthCookie(currentCfg) {
  const configuration = new vrchat.Configuration({
    username: encodeURIComponent(currentCfg.VRChat.user),
    password: encodeURIComponent(currentCfg.VRChat.pass),
    baseOptions: { headers: { 'User-Agent': USER_AGENT } }
  });
  const authApi = new vrchat.AuthenticationApi(configuration);

  let cookie;
  let first;
  try {
    first = await authApi.getCurrentUser();
  } catch (err) {
    console.error('[invCheck] initial getCurrentUser failed:', err.response?.data || err.message);
    throw err;
  }
  if (first?.headers?.['set-cookie']) {
    const found = first.headers['set-cookie'].find((c) => typeof c === 'string' && c.startsWith('auth='));
    if (found) cookie = found.split(';')[0].split('=')[1];
  }

  if (!first?.data?.displayName) {
    const otp = generateOtpToken(currentCfg.VRChat.twofa);
    await authApi.verify2FA({ code: otp });
    const again = await authApi.getCurrentUser();
    if (again?.headers?.['set-cookie']) {
      const found = again.headers['set-cookie'].find((c) => typeof c === 'string' && c.startsWith('auth='));
      if (found) cookie = found.split(';')[0].split('=')[1];
    }
  }

  if (!cookie) {
    if (currentCfg?.VRChat?.authCookie) {
      cookie = currentCfg.VRChat.authCookie;
    } else {
      throw new Error('Failed to obtain auth cookie');
    }
  }

  currentCfg.VRChat.authCookie = cookie;
  saveConfig(currentCfg);
  return cookie;
}

async function ensureAuthCookie(force = false) {
  const current = loadConfig();
  if (!authCookie || force) {
    authCookie = await loginAndGetAuthCookie(current);
  }
  return authCookie;
}

function createAxiosClient() {
  const headers = { 'User-Agent': USER_AGENT };
  if (authCookie) headers.Cookie = `auth=${authCookie}`;
  return axios.create({ baseURL: API_BASE, headers, timeout: 30000 });
}

async function ensureClient(force = false) {
  await ensureAuthCookie(force);
  return createAxiosClient();
}

function parseIdentifier(id) {
  if (!id || typeof id !== 'string') return null;
  if (id.startsWith('prnt_')) {
    return { type: 'print', printId: id };
  }
  if (id.includes('&')) {
    const [usr, inv] = id.split('&').map((part) => part.trim()).filter(Boolean);
    if (usr && inv && usr.startsWith('usr_') && inv.startsWith('inv_')) {
      return { type: 'inventory', userId: usr, inventoryId: inv };
    }
  }
  return null;
}

function isTypeId(value, prefix) {
  return typeof value === 'string' && value.toLowerCase().startsWith(prefix);
}

function canonicalizeType(rawType, payload, identifier) {
  const raw = typeof rawType === 'string' ? rawType.toLowerCase() : '';
  if (raw === 'print' || raw === 'sticker' || raw === 'emoji') {
    return raw;
  }

  const idCandidates = [];
  if (typeof identifier === 'string') idCandidates.push(identifier);
  if (payload && typeof payload === 'object') {
    if (typeof payload.id === 'string') idCandidates.push(payload.id);
    if (typeof payload.itemId === 'string') idCandidates.push(payload.itemId);
  }

  if (idCandidates.some((val) => isTypeId(val, 'prnt_'))) return 'print';
  if (idCandidates.some((val) => isTypeId(val, 'sticker_'))) return 'sticker';
  if (idCandidates.some((val) => isTypeId(val, 'emoji_'))) return 'emoji';

  const metadata = payload && typeof payload === 'object' ? payload.metadata : null;
  if (metadata && typeof metadata === 'object') {
    const templateId =
      (typeof metadata.templateId === 'string' && metadata.templateId) ||
      (typeof metadata.template_id === 'string' && metadata.template_id) ||
      '';
    if (templateId) {
      const lower = templateId.toLowerCase();
      if (lower.includes('sticker')) return 'sticker';
      if (lower.includes('emoji')) return 'emoji';
    }
    const tags = Array.isArray(metadata.tags) ? metadata.tags : [];
    for (const tag of tags) {
      if (typeof tag !== 'string') continue;
      const lower = tag.toLowerCase();
      if (lower === 'sticker') return 'sticker';
      if (lower === 'emoji') return 'emoji';
    }
  }

  if (raw) return raw;
  return 'inventory';
}

function finalizeResult(result, identifier) {
  if (!result || typeof result !== 'object') {
    return result;
  }
  const payload = result.payload && typeof result.payload === 'object' ? result.payload : {};
  const canonical = canonicalizeType(result.type, payload, identifier);
  result.type = canonical;
  payload.itemType = canonical;
  result.payload = payload;
  return result;
}

async function fetchPrint(printId) {
  const client = await ensureClient();
  const url = `/prints/${printId}`;
  console.log(`[invCheck] fetching print -> ${url}`);
  const { data } = await client.get(url);
  const payload = {
    itemType: 'print',
    imageUrl: data?.files?.image ?? null,
    id: data?.id ?? printId,
    ownerId: data?.ownerId ?? data?.authorId ?? null
  };
  return { type: 'print', payload };
}

async function fetchInventory(userId, inventoryId) {
  const client = await ensureClient();
  const url = `/user/${userId}/inventory/${inventoryId}`;
  console.log(`[invCheck] fetching inventory -> ${url}`);
  const { data } = await client.get(url);
  let itemType = 'inventory';
  if (typeof data?.itemType === 'string') {
    const normalized = data.itemType.toLowerCase();
    if (normalized === 'sticker' || normalized === 'emoji') {
      itemType = normalized;
    }
  }
  const payload = {
    itemType,
    imageUrl: data?.imageUrl ?? data?.metadata?.imageUrl ?? null,
    id: data?.id ?? inventoryId,
    ownerId: data?.holderId ?? data?.ownerId ?? userId
  };
  return { type: itemType, payload };
}

function buildCacheKey(parsed) {
  if (!parsed) return null;
  if (parsed.type === 'print') return parsed.printId;
  if (parsed.type === 'inventory') return `${parsed.userId}&${parsed.inventoryId}`;
  return null;
}

async function runFetchAndCache(cacheKey, parsed, rawIdentifier) {
  let result;
  if (parsed.type === 'print') {
    result = await fetchPrint(parsed.printId);
  } else {
    result = await fetchInventory(parsed.userId, parsed.inventoryId);
  }
  finalizeResult(result, rawIdentifier);
  console.log(`[invCheck] type detected: ${result.type}`);
  console.log(
    `[invCheck] fetched type=${result.type} id=${result.payload?.id ?? parsed.printId ?? parsed.inventoryId}`
  );
  const responsePayload = { ok: true, type: result.type, payload: result.payload };
  await writeCachedResult(cacheKey, responsePayload);
  return responsePayload;
}

app.post('/invChk', async (req, res) => {
  const { id } = req.body || {};
  console.log('[invCheck] received id:', id);

  const parsed = parseIdentifier(id);
  if (!parsed) {
    console.warn('[invCheck] unable to parse identifier, skipping');
    return res.status(400).json({ ok: false, error: 'invalid identifier' });
  }

  const cacheKey = buildCacheKey(parsed);
  if (!cacheKey) {
    console.warn('[invCheck] unable to build cache key');
    return res.status(400).json({ ok: false, error: 'unsupported identifier' });
  }

  try {
    const cached = await readCachedResult(cacheKey);
    if (cached) {
      console.log(`[invCheck] cache hit -> ${cacheKey}`);
      if (CACHE_HIT_DELAY_MS > 0) await delay(CACHE_HIT_DELAY_MS);
      return res.json(cached);
    }

    console.log(`[invCheck] cache miss -> ${cacheKey}, enqueueing job`);
    const responsePayload = await scheduleJob(cacheKey, () => runFetchAndCache(cacheKey, parsed, id));
    return res.json(responsePayload);
  } catch (err) {
    console.error('[invCheck] fetch failed:', err?.response?.status || err?.message || err);
    return res.status(502).json({ ok: false, error: 'fetch failed' });
  }
});


const port = process.env.INV_CHECK_PORT || 38062;
app.listen(port, () => {
  console.log(`[invCheck] listening on port ${port}`);
});
