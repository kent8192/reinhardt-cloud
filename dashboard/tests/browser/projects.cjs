const { chromium } = require(process.env.CLOUD_BROWSER_PACKAGES + '/playwright');
const { expect } = require(process.env.CLOUD_BROWSER_PACKAGES + '/playwright/test');
const assert = require('node:assert/strict');

(async () => {
  const executablePath = process.env.CLOUD_BROWSER_EXECUTABLE;
  const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  const errors = [];
  const failed = [];
  try {
    const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, colorScheme: 'light', reducedMotion: 'reduce' });
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    page.on('requestfailed', request => failed.push(request.url() + ': ' + request.failure()?.errorText));
    let release;
    const gate = new Promise(resolve => { release = resolve; });
    await page.route('**/*.wasm', async route => { await gate; await route.continue(); });
    const response = await page.goto(process.env.CLOUD_BROWSER_URL, { waitUntil: 'commit' });
    assert.equal(response.status(), 200);
    assert.equal(new URL(page.url()).pathname, '/login/');
    try {
      await expect(page.getByRole('heading', { name: 'Projects', exact: true })).toBeVisible();
    } catch (error) {
      console.error(JSON.stringify({
        initialText: await page.locator('body').innerText(),
        initialRenderMode: await page.locator('#root').getAttribute('data-render-mode'),
        headings: await page.locator('h1').allTextContents()
      }));
      throw error;
    }
    await page.evaluate(() => { window.serverHeading = document.querySelector('h1'); });
    release();
    await expect(page.locator('#root')).toHaveAttribute('data-render-mode', 'hydrated', { timeout: 30000 });
    assert.equal(await page.evaluate(() => window.serverHeading === document.querySelector('h1')), true);
    await expect(page.locator('link[rel=stylesheet]')).toHaveCount(1);
    await page.getByRole('button', { name: '日本語', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'プロジェクト', exact: true })).toBeVisible();
    await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
    await page.getByRole('button', { name: 'English', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading', { name: 'Projects', exact: true })).toBeVisible();
    await page.emulateMedia({ colorScheme: 'dark' });
    assert.equal(await page.evaluate(() => getComputedStyle(document.querySelector('#root > div')).backgroundColor), 'rgb(17, 24, 39)');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'light' });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
    await page.getByLabel('Email', { exact: true }).fill('browser-owner@example.test');
    await page.getByLabel('Password', { exact: true }).fill('a-long-browser-password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Choose an organization', exact: true })).toBeVisible();
    await expect(page.locator('#root')).toHaveAttribute('data-render-mode', 'hydrated');
    const organizationLink = page.getByRole('link', { name: 'browser-owner@example.test', exact: true });
    const organization = new URL(await organizationLink.getAttribute('href'), page.url()).pathname.split('/')[2];
    await page.evaluate(() => { window.navigationDocument = document; });
    const projectsRequest = page.waitForResponse(response => response.url().endsWith('/api/server_fn/projects/') && response.request().method() === 'POST');
    await organizationLink.click();
    const projectsResponse = await projectsRequest;
    if (projectsResponse.status() !== 200) {
      const headers = await projectsResponse.request().allHeaders();
      console.error(JSON.stringify({ status: projectsResponse.status(), body: await projectsResponse.text(), origin: headers.origin, csrfPresent: Boolean(headers['x-csrftoken']), csrfLength: headers['x-csrftoken']?.length, metaCount: await page.locator('meta[name=csrf-token]').count(), cookieNames: (await context.cookies()).map(cookie=>cookie.name) }));
    }
    assert.equal(projectsResponse.status(), 200);
    await expect(page.getByRole('link', { name: 'Browser App', exact: true })).toBeVisible();
    await expect(page.locator('#root')).toHaveAttribute('data-render-mode', 'routed');
    assert.equal(await page.evaluate(() => window.navigationDocument === document), true);
    assert.equal(new URL(page.url()).pathname, '/organizations/' + organization + '/projects/');
    const own = await context.request.get(process.env.CLOUD_BROWSER_URL + 'api/v1/projects/?organization=' + organization);
    assert.equal(own.status(), 200);
    assert.equal((await own.json())[0].name, 'Browser App');
    const other = await context.request.get(process.env.CLOUD_BROWSER_URL + 'api/v1/projects/?organization=00000000-0000-0000-0000-000000000099');
    assert.equal(other.status(), 403);
    await page.getByRole('button', { name: '日本語', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'プロジェクト', exact: true })).toBeVisible();
    const projectRequest = page.waitForResponse(response => response.url().endsWith('/api/server_fn/project/') && response.request().method() === 'POST');
    await page.getByRole('link', { name: 'Browser App', exact: true }).click();
    assert.equal((await projectRequest).status(), 200);
    assert.equal(await page.evaluate(() => window.navigationDocument === document), true);
    await expect(page.getByRole('heading', { name: 'Browser App', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'ステージング', exact: true })).toBeVisible();
    await expect(page.getByText('スケーリング: 結果が不確定', { exact: true })).toBeVisible();
    await expect(page.getByRole('status')).toHaveText('クラスタの状態確認が完了するまで、次の変更を待機します。');
    await expect(page.locator('#root')).toHaveAttribute('data-render-mode', 'routed');
    await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
    const staging = page.locator('article').filter({ has: page.getByRole('heading', { name: 'ステージング', exact: true }) });
    await expect(staging.locator('dd')).toHaveText(['1', '2']);
    await expect(staging.getByRole('button', { name: 'スケーリング', exact: true })).toBeDisabled();
    await expect(staging.locator('fieldset')).toHaveCSS('opacity', '0.6');
    await page.goBack();
    await expect(page.getByRole('link', { name: 'Browser App', exact: true })).toBeVisible();
    await page.goForward();
    await expect(page.getByRole('heading', { name: 'Browser App', exact: true })).toBeVisible();
    assert.equal(await page.evaluate(() => window.navigationDocument === document), true);
    const preview = page.locator('article').filter({ has: page.getByRole('heading', { name: 'プレビュー', exact: true }) });
    await preview.getByLabel('設定レプリカ数', { exact: true }).fill('2');
    const accepted = page.waitForResponse(response => response.url().endsWith('/api/server_fn/runtime/') && response.request().method() === 'POST');
    await preview.getByRole('button', { name: 'スケーリング', exact: true }).click();
    const acceptedResponse = await accepted.catch(async error => {
      console.error(JSON.stringify({ operationText: await preview.innerText(), replicas: await preview.getByLabel('設定レプリカ数', { exact: true }).inputValue(), errors, failed }));
      throw error;
    });
    assert.equal(acceptedResponse.status(), 200);
    const input = acceptedResponse.request().postDataJSON();
    const progress = await acceptedResponse.json();
    const mutationHeaders = { Origin: new URL(page.url()).origin, 'X-CSRFToken': await page.locator('meta[name=csrf-token]').getAttribute('content') };
    const replayOperation = await context.request.post(new URL('/api/server_fn/runtime/', page.url()).href, { headers: mutationHeaders, data: input });
    assert.equal(replayOperation.status(), 200);
    assert.deepEqual(await replayOperation.json(), progress);
    const conflictOperation = await context.request.post(new URL('/api/server_fn/runtime/', page.url()).href, { headers: mutationHeaders, data: { input: { ...input.input, change: { ...input.input.change, replicas: 3 } } } });
    assert.equal(conflictOperation.status(), 409);
    await expect(preview.locator('dd')).toHaveText(['1', '2']);
    await expect(preview.getByLabel('設定レプリカ数', { exact: true })).toHaveValue('2');
    await expect(preview.getByRole('button', { name: 'スケーリング', exact: true })).toBeDisabled();
    assert.equal(await page.evaluate(() => window.navigationDocument === document), true);
    await page.reload();
    await expect(page.getByRole('heading', { name: 'ステージング', exact: true })).toBeVisible();
    await expect(page.locator('#root')).toHaveAttribute('data-render-mode', 'hydrated');
    await page.getByRole('button', { name: 'English', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Staging', exact: true })).toBeVisible();
    await expect(page.getByText('Scale: Outcome uncertain', { exact: true })).toBeVisible();
    await page.emulateMedia({ colorScheme: 'dark' });
    assert.equal(await page.evaluate(() => getComputedStyle(document.querySelector('article')).borderTopColor), 'rgb(51, 65, 85)');
    if (process.env.CLOUD_BROWSER_SCREENSHOT) {
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.emulateMedia({ colorScheme: 'light' });
      await page.screenshot({ path: process.env.CLOUD_BROWSER_SCREENSHOT, fullPage: true });
    }
    const activeCookie = (await context.cookies()).find(cookie => cookie.name === 'cloud_session');
    assert.equal(activeCookie.httpOnly, true);
    await page.getByRole('button', { name: 'Sign out', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Sign in to your organization', exact: true })).toBeVisible();
    await expect(page.locator('#root')).toHaveAttribute('data-render-mode', 'hydrated');
    const replay = await context.request.get(process.env.CLOUD_BROWSER_URL + 'api/v1/organizations/', { headers: { Cookie: activeCookie.name + '=' + activeCookie.value } });
    assert.equal(replay.status(), 401);
    assert.deepEqual(errors, []);
    assert.deepEqual(failed, []);
    console.log('Dashboard browser checks passed');
  } finally {
    if (errors.length) console.error(JSON.stringify({ browserErrors: errors }));
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
