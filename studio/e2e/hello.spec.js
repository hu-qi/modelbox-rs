import { test, expect } from '@playwright/test';

test('real Rust hello graph validates, edits, executes, and reloads', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Rust FlowUnit Registry 已连接');
  await expect(page.locator('.react-flow__node')).toHaveCount(4);

  await page.getByRole('button', { name: '校验 Graph' }).click();
  await expect(page.getByRole('status')).toContainText('valid graph: hello');

  await page.locator('.react-flow__node').filter({ hasText: 'text.prefix' }).click();
  await page.locator('textarea').fill('{"prefix":"E2E: "}');
  await page.getByRole('button', { name: '执行 Graph' }).click();
  await expect(page.getByRole('status')).toContainText('未应用修改');
  await page.getByRole('button', { name: '应用节点配置' }).click();

  await page.getByRole('button', { name: '执行 Graph' }).click();
  await expect(page.getByRole('status')).toContainText('E2E: HELLO MODELBOX');

  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('button', { name: '保存画布' }).click()
  ]);
  const path = await download.path();
  await page.locator('input[type=file]').setInputFiles(path);
  await expect(page.getByRole('status')).toContainText('已导入');
  await page.getByRole('button', { name: '执行 Graph' }).click();
  await expect(page.getByRole('status')).toContainText('E2E: HELLO MODELBOX');
});

test('real Rust CLI reports invalid edited graph', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Registry 已连接');
  await page.locator('.react-flow__node').filter({ hasText: 'text.uppercase' }).click();
  await page.getByRole('button', { name: '删除节点' }).click();
  await page.getByRole('button', { name: '校验 Graph' }).click();
  await expect(page.getByRole('status')).toContainText('失败');
});
