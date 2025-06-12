import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname  = path.dirname(__filename);
const TOKEN_FILE = path.join(__dirname, 'token.txt');

const BASE_URL = 'http://127.0.0.1:3010';

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
  const decibels = Math.floor(Math.random() * 11) + 55; // 55-65

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
  }, 1000);
})();