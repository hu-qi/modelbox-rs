import { test, expect } from '@playwright/test';

test('real Rust hello graph validates, edits, executes, and reloads', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Rust FlowUnit Registry 已连接');
  await expect(page.locator('.react-flow__node')).toHaveCount(4);

  await page.getByRole('button', { name: '校验 Graph' }).click();
  await expect(page.getByText('valid graph: hello', { exact: false })).toBeVisible();

  await page.locator('.react-flow__node').filter({ hasText: 'text.prefix' }).click();
  await page.locator('textarea').fill('{"prefix":"E2E: "}');
  await page.getByRole('button', { name: '执行 Graph' }).click();
  await expect(page.getByRole('status')).toContainText('未应用修改');
  await page.getByRole('button', { name: '应用节点配置' }).click();

  await page.getByRole('button', { name: '执行 Graph' }).click();
  await expect(page.getByText('E2E: HELLO MODELBOX', { exact: false })).toBeVisible();

  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('button', { name: '保存画布' }).click()
  ]);
  const path = await download.path();
  await page.locator('input[type=file]').setInputFiles(path);
  await expect(page.getByRole('status')).toContainText('已导入');
  await page.getByRole('button', { name: '执行 Graph' }).click();
  await expect(page.getByText('E2E: HELLO MODELBOX', { exact: false })).toBeVisible();
});

test('real Rust CLI reports invalid edited graph', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.locator('.react-flow__node').filter({ hasText: 'text.uppercase' }).click();
  await page.getByRole('button', { name: '删除节点' }).click();
  await page.getByRole('button', { name: '校验 Graph' }).click();
  await expect(page.getByRole('status')).toContainText('失败');
  await expect(page.getByRole('alert')).toContainText('graph');
});

test('profiled node metrics come from real Rust Runtime', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.getByRole('button', { name: '分析节点耗时' }).click();
  await expect(page.getByLabel('节点处理耗时')).toContainText('upper');
  await expect(page.getByLabel('节点处理耗时')).toContainText('prefix');
  await expect(page.getByLabel('节点处理耗时')).toContainText('result');
  await expect(page.getByLabel('节点处理耗时')).toContainText('不含调度及队列');
  await expect(page.locator('.react-flow__node').filter({ hasText: 'text.uppercase' })).toContainText('已处理 1 次');
  await expect(page.locator('.react-flow__node').filter({ hasText: 'core.input' })).toContainText('源节点');
  await page.getByRole('textbox', { name: 'Graph name' }).fill('modified');
  await expect(page.locator('.execution-state')).toHaveCount(0);
});

test('structured Runtime failure selects node and stale annotation clears on graph change', async ({ page }) => {
  await page.route('**/api/run', async route => {
    await route.fulfill({status:422,contentType:'application/json',body:JSON.stringify({
      error:'execution error at node upper: deliberate failure',node_id:'upper'
    })});
  });
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.getByRole('button',{name:'执行 Graph'}).click();
  await expect(page.getByLabel('故障节点详情')).toContainText('upper');
  await page.getByRole('button',{name:'选择故障节点'}).click();
  await expect(page.locator('.react-flow__node.selected')).toContainText('text.uppercase');
  await page.getByRole('textbox',{name:'Graph name'}).fill('changed');
  await expect(page.getByLabel('故障节点详情')).toHaveCount(0);
  await expect(page.locator('.unit-failed')).toHaveCount(0);
});

test('real Rust frame timeline links measured events back to canvas nodes', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.getByRole('button', {name:'查看逐帧轨迹'}).click();
  const timeline = page.getByLabel('逐帧运行轨迹');
  await expect(timeline).toContainText('已保留 3 条');
  await expect(timeline).toContainText('upper · 帧 0');
  await expect(timeline).toContainText('prefix · 帧 0');
  await expect(timeline).toContainText('result · 帧 0');
  await timeline.getByRole('button', {name:/upper · 帧 0/}).click();
  await expect(page.locator('.react-flow__node.selected')).toContainText('text.uppercase');
  await page.getByRole('textbox', {name:'Graph name'}).fill('changed');
  await expect(page.getByLabel('逐帧运行轨迹')).toHaveCount(0);
});
