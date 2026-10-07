import { expect, test } from '@playwright/test'

// UTC timestamps cross midnight locally, while remaining on the same UTC date.
test.use({ timezoneId: 'Asia/Shanghai' })

for (const scenario of [
  { name: 'same day', start: '2026-10-07T12:30:00+08:00', end: '2026-10-07T13:30:00+08:00', dated: false },
  { name: 'local midnight', start: '2026-10-07T23:30:00+08:00', end: '2026-10-08T00:30:00+08:00', dated: true },
  { name: 'new year', start: '2026-12-31T23:30:00+08:00', end: '2027-01-01T00:30:00+08:00', dated: true },
]) {
  test(`charts distinguish ${scenario.name} in the local timezone`, async ({ page, request }, info) => {
    const snapshot = await (await request.get('/api/snapshot')).json()
    const start = Date.parse(scenario.start)
    const end = Date.parse(scenario.end)
    snapshot.now_ms = end
    snapshot.trend = Array.from({ length: 5 }, (_, i) => ({
      time_ms: start + (end - start) * i / 4,
      ttft_ms: 2000,
      decode_tps: 25,
      input_tokens: 24000,
      cached_input_tokens: 18000,
      output_tokens: 1200,
      reasoning_output_tokens: 800,
      http_failure_percent: 10,
      websocket_send_failure_percent: 1,
    }))
    await page.route('**/api/snapshot?*', route => route.fulfill({ json: snapshot }))
    await page.goto('/')
    const charts = page.locator('.trend-chart')
    await expect(charts).toHaveCount(4)
    for (const language of ['zh-CN', 'en-US']) {
      if (language === 'en-US') {
        await page.getByRole('combobox', { name: 'Language / 语言' }).click()
        await page.getByRole('option', { name: 'English', exact: true }).click()
      }
      const axisFormat = new Intl.DateTimeFormat(language, {
        timeZone: 'Asia/Shanghai',
        ...(scenario.dated ? { month: '2-digit', day: '2-digit' } as const : {}),
        ...(scenario.name === 'new year' ? { year: 'numeric' } as const : {}),
        hour: '2-digit', minute: '2-digit', hour12: false,
      })
      const tooltipFormat = new Intl.DateTimeFormat(language, {
        timeZone: 'Asia/Shanghai', year: 'numeric', month: '2-digit', day: '2-digit',
        hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false,
      })
      for (const chart of await charts.all()) {
        const ticks = chart.locator('.recharts-xAxis-tick-labels .recharts-cartesian-axis-tick-value')
        await expect(ticks.first()).toHaveText(axisFormat.format(start))
        await expect(ticks.last()).toHaveText(axisFormat.format(end))
        // Keyboard navigation activates the exact first data point on every chart.
        await chart.locator('svg.recharts-surface').focus()
        await page.keyboard.press('ArrowRight')
        await page.keyboard.press('ArrowLeft')
        await expect(chart.locator('.recharts-tooltip-wrapper')).toContainText(tooltipFormat.format(start))
        await page.getByRole('heading').first().click()
      }
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBeTruthy()
    }
    if (scenario.name === 'local midnight') {
      await page.screenshot({ path: `/tmp/codex-speed-chart-dates-${info.project.name}.png`, fullPage: true })
    }
  })
}
