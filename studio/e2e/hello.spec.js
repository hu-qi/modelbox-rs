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

test('actual Rust channel API diagnostic values are shown and invalidated', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.getByRole('button', {name:'查看通道API计时'}).click();
  const panel=page.getByLabel('通道API计时');
  await expect(panel).toContainText('source');
  await expect(panel).toContainText('upper');
  await expect(panel).toContainText('接收');
  await expect(panel).toContainText('发送');
  await expect(panel).toContainText('背压');
  await page.getByRole('textbox', {name:'Graph name'}).fill('changed');
  await expect(page.getByLabel('通道API计时')).toHaveCount(0);
});

test('real Rust Edge tracing shows link identity and invalidates stale statistics', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.getByRole('button',{name:'分析逐边交付'}).click();
  const panel=page.getByLabel('逐边传输诊断');
  await expect(panel).toContainText('已识别 3 条 Edge');
  await expect(panel).toContainText('已保留 3 条逐帧交付事件');
  await expect(panel).toContainText('source.out → upper.in');
  await expect(panel).toContainText('upper.out → prefix.in');
  await expect(panel).toContainText('不能视为精确的队列驻留时间');
  await panel.locator('.edge-entry').first().click();
  await expect(page.locator('.react-flow__node.selected')).toContainText('text.uppercase');
  await page.getByRole('textbox',{name:'Graph name'}).fill('changed');
  await expect(page.getByLabel('逐边传输诊断')).toHaveCount(0);
});

test('genuine legacy TOML graph is mapped, Rust validated, then executed', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.locator('input[type=file]').setInputFiles('../examples/legacy_modelbox_hello.toml');
  await expect(page.getByRole('status')).toContainText('已迁移兼容子集');
  await expect(page.getByRole('status')).toContainText('Rust 校验通过');
  await expect(page.locator('.react-flow__node')).toHaveCount(4);
  await page.getByRole('button', {name:'执行 Graph'}).click();
  await expect(page.getByText('ModelBox Rust says: HELLO MODELBOX', {exact:false})).toBeVisible();
});

test('unknown legacy plugins fail closed without mutating Studio graph', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await expect(page.locator('.react-flow__node')).toHaveCount(4);
  await page.locator('input[type=file]').setInputFiles({
    name:'unsupported.dot', mimeType:'text/plain',
    buffer:Buffer.from('digraph legacy { src[type=input]; resize[type=flowunit,flowunit=resize,device=cuda]; dst[type=output]; src -> resize:in; resize:out -> dst; }')
  });
  await expect(page.getByRole('status')).toContainText('unsupported legacy FlowUnit');
  const report = page.getByLabel('旧版迁移兼容性分析');
  await expect(report).toContainText('unmapped_flowunit');
  await expect(report).toContainText('device');
  await expect(report).toContainText('原 Graph 未被替换');
  await expect(page.locator('.react-flow__node')).toHaveCount(4);
  await expect(page.getByRole('textbox',{name:'Graph name'})).toHaveValue('hello');
});

test('late Registry response must not overwrite an import error', async ({ page }) => {
  let releaseCatalog;
  const held = new Promise(resolve => { releaseCatalog = resolve; });
  await page.route('**/api/catalog', async route => {
    await held;
    await route.continue();
  });
  await page.goto('/');
  await page.locator('input[type=file]').setInputFiles({
    name:'blocked.dot', mimeType:'text/plain',
    buffer:Buffer.from('digraph demo { src[type=input]; bad[type=flowunit,flowunit=missing_plugin]; dst[type=output]; src -> bad:in; bad:out -> dst; }')
  });
  await expect(page.getByRole('status')).toContainText('unsupported legacy FlowUnit');
  const response = page.waitForResponse(r => r.url().includes('/api/catalog'));
  releaseCatalog();
  await response;
  await expect(page.getByRole('status')).toContainText('unsupported legacy FlowUnit');
  await expect(page.getByLabel('旧版迁移兼容性分析')).toContainText('unmapped_flowunit');
});

test('reviewed concat port mapping executes real Rust fan-in after DOT import', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.locator('input[type=file]').setInputFiles('../examples/legacy_modelbox_concat.dot');
  await expect(page.getByRole('status')).toContainText('Rust 校验通过');
  await expect(page.locator('.react-flow__node')).toHaveCount(5);
  await page.getByRole('button',{name:'执行 Graph'}).click();
  await expect(page.getByText('HELLO MODELBOXR: hello modelbox',{exact:false})).toBeVisible();
});
