import { defineConfig, devices } from '@playwright/test'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const data = mkdtempSync(join(tmpdir(), 'codex-speed-e2e-'))
export default defineConfig({
  testDir: './tests',
  workers: 1,
  fullyParallel: false,
  reporter: 'list',
  use: { locale: 'zh-CN', reducedMotion: 'reduce', baseURL: 'http://127.0.0.1:14318', trace: 'retain-on-failure', screenshot: 'only-on-failure' },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 1040 } } },
    { name: 'laptop', use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 900 } } },
  ],
  webServer: { command: `../target/debug/codex-speed --port 14318 --data-dir "${data}"`, url: 'http://127.0.0.1:14318/api/health', reuseExistingServer: false },
})
