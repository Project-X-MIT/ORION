# Release UAT matrix

The repository-level UAT matrix exercises the public authentication journeys
and the authenticated application shell with a deterministic synthetic API.
It checks keyboard/focusable form controls, axe accessibility violations, and
horizontal overflow at desktop and mobile viewports. The Playwright projects
cover:

| Project | Browser/device | Viewport purpose |
| --- | --- | --- |
| `chromium-desktop` | Chromium / Desktop Chrome | baseline desktop |
| `firefox-desktop` | Firefox / Desktop Firefox | supported desktop browser |
| `webkit-desktop` | WebKit / Desktop Safari | supported desktop browser |
| `chromium-mobile` | Chromium / iPhone 13 emulation | responsive mobile |

Run the matrix locally from a disposable checkout:

```bash
npm ci --ignore-scripts
npm install --no-save --package-lock=false --ignore-scripts @rollup/rollup-linux-x64-gnu@4.62.4
(cd frontend && npx playwright install chromium firefox webkit)
npm run test:e2e --workspace frontend -- --grep "release UAT matrix"
```

The tests mock only the API responses needed for the shell journey and use
synthetic credentials. They do not constitute product-owner UAT or a production
browser support sign-off. A release record must attach the staging fixture,
run ID, accessibility report, and product/security/operations approvals before
the release acceptance criterion is checked.
