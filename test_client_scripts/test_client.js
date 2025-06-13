import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const TOKEN_FILE = path.join(__dirname, 'token.txt');

const BASE_URL = 'http://127.0.0.1:3010';

// --- Decibel simulation state ---
let t = 0; // Time step

// Sine parameters that will change over time
let sineAmplitude = 10 + Math.random() * 10; // 10–20
let sineFrequency = 20 + Math.random() * 40; // 20–60
let sinePhase = Math.random() * Math.PI * 2;

// Random walk offset
let randomOffset = 0;

function randomizeSineParams() {
  sineAmplitude = 10 + Math.random() * 10; // 10–20
  sineFrequency = 20 + Math.random() * 40; // 20–60
  sinePhase = Math.random() * Math.PI * 2;
}

// Change sine parameters every 200 steps for more randomness
function maybeRandomizeSine() {
  if (t % 200 === 0) randomizeSineParams();
}

function getNextDecibels() {
  maybeRandomizeSine();

  // Sine wave for smooth periodic fluctuation
  const sine =
    Math.sin((t + sinePhase) / sineFrequency) * sineAmplitude;

  // Small random walk for realism, but keep it bounded
  randomOffset += (Math.random() - 0.5) * 0.5;
  // Keep randomOffset within -5 to +5
  randomOffset = Math.max(-5, Math.min(5, randomOffset));

  // Base value in the middle of the range
  const base = 65;

  // Calculate next value
  let decibels = base + sine + randomOffset;

  // Clamp to 50-80
  decibels = Math.max(50, Math.min(80, decibels));
  t++;

  // Round to 1 decimal place
  return Math.round(decibels * 10) / 10;
}

async function fetchToken() {
  const res = await fetch(`${BASE_URL}/api/auth`);
  if (!res.ok) throw new Error(`auth request failed (${res.status})`);

  // prefer header, fallback to json.token
  let token = res.headers.get('x-device-token');
  if (!token) {
    const json = await res.json().catch(() => null);
    token = json?.token;
  }

  if (!token) throw new Error('no token in auth response');

  fs.writeFileSync(TOKEN_FILE, token, 'utf8');
  console.log('obtained new token');
  return token;
}

async function getToken() {
  if (fs.existsSync(TOKEN_FILE)) {
    const token = fs.readFileSync(TOKEN_FILE, 'utf8').trim();
    if (token) return token;
  }
  return fetchToken();
}

async function postLog(token) {
  const decibels = getNextDecibels();

  const res = await fetch(`${BASE_URL}/api/logs`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${token}`,
      'content-type': 'application/json'
    },
    body: JSON.stringify({ decibels })
  });

  if (res.ok) {
    console.log(`sent { decibels: ${decibels} }`);
    return token;
  }

  if (res.status === 401) {
    console.warn('token rejected – refreshing');
    fs.rmSync(TOKEN_FILE, { force: true });
    return fetchToken();
  }

  console.error(`log post failed (${res.status})`);
  return token;
}

(async () => {
  let token = await getToken();

  setInterval(async () => {
    try {
      token = await postLog(token);
    } catch (err) {
      console.error('unexpected error:', err);
    }
  }, 100);
})();