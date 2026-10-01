import { expect, test } from '@playwright/test'
import AxeBuilder from '@axe-core/playwright'

function payload(model: string, time: number, ttft: number, tbt: number) {
  const histogram = (name: string, sum: number, tokenType?: string) => ({
    name, histogram: { aggregationTemporality: 1, dataPoints: [{
      attributes: [{ key: 'model', value: { stringValue: model } }, ...(tokenType ? [{ key: 'token_type', value: { stringValue: tokenType } }] : [])],
      startTimeUnixNano: String(BigInt(time - 1000) * 1000000n), timeUnixNano: String(BigInt(time) * 1000000n),
      count: '1', sum, explicitBounds: [1000, 5000, 100000], bucketCounts: sum <= 1000 ? ['1', '0', '0', '0'] : sum <= 5000 ? ['0', '1', '0', '0'] : ['0', '0', '1', '0'],
    }] },
  })
  return { resourceMetrics: [{ scopeMetrics: [{ metrics: [
    histogram('codex.responses_api_engine_service_ttft.duration_ms', ttft),
    histogram('codex.responses_api_engine_service_tbt.duration_ms', tbt),
    histogram('codex.turn.token_usage', 24000, 'input'),
    histogram('codex.turn.token_usage', 18000, 'cached_input'),
    histogram('codex.turn.token_usage', 1200, 'output'),
  ] }] }] }
}

test('setup, live metrics, filters and accessible responsive layout', async ({ page, request }, info) => {
  const errors: string[] = []
  page.on('pageerror', error => errors.push(error.message))
  await page.goto('/')
  await expect(page.getByRole('heading', { name: '服务表现，一目了然。' })).toBeVisible()
  await page.getByRole('button', { name: '连接 Codex' }).click()
  await expect(page.getByRole('dialog')).toContainText('OTEL_METRIC_EXPORT_INTERVAL=1000')
  await expect(page.getByRole('dialog')).toContainText('--enable runtime_metrics')
  await page.getByRole('button', { name: 'Close', exact: true }).click()
  await expect(page.locator('[data-slot=dialog-content]')).toHaveCount(0)
  const model = `gpt-example-${info.project.name}`
  const now = Date.now()
  for (let i = 0; i < 28; i++) {
    const response = await request.post('/v1/metrics', { data: payload(model, now - (27 - i) * 60000, 2100 + Math.sin(i / 3) * 450, 45 + Math.cos(i / 4) * 8) })
    expect(response.ok()).toBeTruthy()
  }
  await expect(page.getByRole('button', { name: model, exact: true })).toBeVisible()
  await page.getByRole('button', { name: model, exact: true }).click()
  await expect(page.locator('#model-select')).toContainText(model)
  await page.locator('#model-select').click()
  await expect(page.getByRole('option', { name: '全部模型', exact: true })).toHaveCount(0)
  await expect(page.getByRole('option', { name: model, exact: true })).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(page.locator('.metrics-grid')).toContainText('28 个有效样本')
  await expect(page.locator('.metrics-grid')).toContainText('33.6K')
  const usage = page.getByRole('region', { name: 'Token 用量趋势' })
  await expect(usage.getByText('输入', { exact: true })).toBeVisible()
  await expect(usage.getByText('缓存输入', { exact: true })).toBeVisible()
  await expect(usage.getByText('输出', { exact: true })).toBeVisible()
  await expect(usage.locator('.recharts-line')).toHaveCount(3)
  await usage.getByRole('button', { name: '输出', exact: true }).click()
  await expect(usage.getByRole('button', { name: '输出', exact: true })).toHaveAttribute('aria-pressed', 'true')
  await expect(usage.locator('.recharts-line')).toHaveCount(1)
  await usage.getByRole('button', { name: '全部', exact: true }).click()
  await expect(usage.locator('.recharts-line')).toHaveCount(3)
  await page.getByRole('button', { name: '15 分钟', exact: true }).click()
  await expect.poll(async () => page.locator('.metrics-grid').innerText()).not.toContain('28 个有效样本')
  await page.getByRole('button', { name: '1 小时', exact: true }).click()
  await expect(page.locator('.metrics-grid')).toContainText('28 个有效样本')
  const next = payload(model, now + 1, 3200, 50)
  await request.post('/v1/metrics', { data: next })
  await expect(page.locator('.metrics-grid')).toContainText('29 个有效样本')
  await request.post('/v1/metrics', { data: next })
  await expect(page.locator('.metrics-grid')).toContainText('29 个有效样本')
  await page.getByRole('button', { name: '指标说明', exact: true }).click()
  await expect(page.getByRole('dialog')).toContainText('1000 ÷ 平均 Service TBT')
  expect((await new AxeBuilder({ page }).analyze()).violations.map(v => ({id:v.id, nodes:v.nodes.map(n => n.failureSummary)}))).toEqual([])
  await page.getByRole('button', { name: 'Close', exact: true }).click()
  await expect(page.locator('[data-slot=dialog-content]')).toHaveCount(0)
  expect((await new AxeBuilder({ page }).analyze()).violations.map(v => ({id:v.id, nodes:v.nodes.map(n => n.failureSummary)}))).toEqual([])
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBeTruthy()
  expect(errors).toEqual([])
  await page.getByRole('heading', { name: '服务表现，一目了然。' }).click()
  await page.evaluate(() => window.scrollTo(0, 0))
  await page.screenshot({ path: `../docs/assets/web-${info.project.name}.png`, fullPage: true })
})


test('copy command works without Clipboard API and offers manual selection when blocked', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await page.goto('/')
  await page.getByRole('button', { name: '连接 Codex' }).click()
  const command = await page.locator('.command-block code').innerText()
  await page.evaluate(() => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined })
  })
  await page.getByRole('button', { name: '复制命令', exact: true }).click()
  await expect(page.getByRole('button', { name: '已复制', exact: true })).toBeVisible()
  await expect(page.getByRole('alert')).toHaveCount(0)
  // Read the real clipboard using the prototype after forcing the HTTP fallback.
  expect(await page.evaluate(() => Object.getOwnPropertyDescriptor(Navigator.prototype, 'clipboard')!.get!.call(navigator).readText())).toBe(command)
  await page.evaluate(() => { document.execCommand = () => false })
  await page.getByRole('button', { name: '已复制', exact: true }).click()
  await expect(page.getByRole('alert')).toContainText('已选中命令')
  expect(await page.evaluate(() => window.getSelection()?.toString())).toBe(command)
  await expect(page.locator('.command-block textarea')).toHaveCount(0)
})
