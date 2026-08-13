import http from 'k6/http';
import { check } from 'k6';

export const options = {
  scenarios: { leaderboard: { executor: 'constant-arrival-rate', rate: 50, timeUnit: '1s', duration: '30s', preAllocatedVUs: 20, maxVUs: 100 } },
  thresholds: { http_req_failed: ['rate<0.01'], http_req_duration: ['p(95)<500', 'p(99)<1000'] },
};

export default function () {
  const response = http.get(`${__ENV.ORION_BASE_URL || 'http://127.0.0.1:3000'}/api/v1/leaderboard?limit=100`);
  check(response, { 'leaderboard responds': (value) => [200, 401, 404].includes(value.status) });
}
