import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  scenarios: { peak: { executor: 'constant-vus', vus: 20, duration: '30s' } },
  thresholds: { http_req_failed: ['rate<0.01'], http_req_duration: ['p(95)<500', 'p(99)<1000'] },
};

const base = __ENV.ORION_BASE_URL || 'http://127.0.0.1:3000';
const runId = (__ENV.RUN_ID || Date.now().toString(36))
  .replace(/[^a-z0-9]/gi, '')
  .slice(-8);

export default function () {
  const live = http.get(`${base}/health/live`);
  check(live, { 'live is 200': (response) => response.status === 200 });
  const email = `load-${runId}-${__VU}-${__ITER}@synthetic.invalid`;
  const register = http.post(`${base}/api/v1/auth/register`, JSON.stringify({ email, username: `load_${runId}_${__VU}_${__ITER}`, password: 'SyntheticLoadPassword123!', display_name: 'Synthetic' }), { headers: { 'Content-Type': 'application/json' } });
  check(register, { 'register is accepted or conflict': (response) => [200, 201, 409].includes(response.status) });
  sleep(0.1);
}
