// Production UI, real HTTP/CORS, synthetic credentials; no request interception.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const http = require('node:http');

module.exports = async function checkRemoteErrors(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  const report = {checks: [], pageErrors: [], consoleContainsKey: false};
  const key = 'synthetic-nonstandard-credential-37fc8d';
  const requests = [], held = [];
  let mode = 'hold401', timedOutSocketClosed = false;
  const server = http.createServer(async (req, res) => {
    const request = {method: req.method, path: req.url}; requests.push(request);
    if (!req.url.startsWith('/cors')) {
      res.setHeader('Access-Control-Allow-Origin', '*');
      res.setHeader('Access-Control-Allow-Headers', 'content-type,authorization');
      res.setHeader('Access-Control-Allow-Methods', 'GET,POST,OPTIONS');
    }
    if (req.method !== 'POST') {res.end('{}'); return;}
    let text = ''; for await (const chunk of req) text += chunk;
    const body = JSON.parse(text);
    const credential = String(req.headers.authorization || '').replace(/^Bearer /, '');
    request.probeOnly = body.messages?.length === 1 && body.messages[0].content === 'Reply with just the two letters OK.' && body.max_tokens === 8;
    request.keyMatches = req.headers.authorization === `Bearer ${key}`;
    res.setHeader('Content-Type', 'application/json');
    if (mode === 'timeout') {
      res.writeHead(200); res.write('{"choices":[');
      res.on('close', () => {timedOutSocketClosed = true;});
      return;
    }
    const reply = () => {
      if (mode === 'hold401' || mode === '401') {res.statusCode = 401; res.end(JSON.stringify({error: {message: `Invalid credential ${credential} ${'x'.repeat(500)}`}})); return;}
      if (mode === 'notJson') {res.end(`Not JSON: ${credential}`); return;}
      if (mode === 'missing') {res.end('{}'); return;}
      const content = mode === 'null' ? null : mode === 'object' ? {unexpected: true} : mode === 'empty' ? '' : mode === 'echo' ? credential : 'OK';
      res.end(JSON.stringify({choices: [{message: {content}}]}));
    };
    if (mode === 'hold401' || mode === 'holdOk') held.push(reply);
    else reply();
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  const endpoint = `http://127.0.0.1:${port}/v1`;
  const check = (ok, label) => {assert.ok(ok, label); report.checks.push(label); console.log(`PASS: ${label}`);};
  await page.addInitScript(() => localStorage.setItem('boardcast.lang', 'en'));
  page.on('pageerror', error => report.pageErrors.push(error.message));
  page.on('console', message => {if (message.text().includes(key)) report.consoleContainsKey = true;});
  const settings = () => page.getByRole('dialog', {name: 'Settings', exact: true});
  const url = () => settings().getByPlaceholder('https://api.openai.com/v1');
  const model = () => settings().getByPlaceholder('e.g. gpt-4o-mini, deepseek-chat, qwen-plus');
  const password = () => settings().locator('input[type=password]');
  const test = () => settings().getByRole('button', {name: 'Test connection', exact: true}).click();
  const errorText = async () => {await settings().locator('.meta.err').waitFor({timeout: 40_000}); return settings().locator('.meta.err').innerText();};
  const waitHeld = async () => {for (let i = 0; i < 100 && !held.length; i++) await page.waitForTimeout(25); assert.ok(held.length);};
  const saved = () => page.evaluate(() => JSON.parse(localStorage.getItem('boardcast.remote') || '{}'));
  const fieldsLocked = async () => (await Promise.all([url(), model(), password()].map(field => field.evaluate(el => el.readOnly || el.disabled)))).every(Boolean);
  try {
    await fs.mkdir(evidence, {recursive: true});
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    report.productionScripts = await page.locator('script[src]').evaluateAll(nodes => nodes.map(node => node.src));
    await page.getByRole('button', {name: 'Settings', exact: true}).click();
    await settings().getByRole('tab', {name: 'Remote & privacy', exact: true}).click();
    await url().fill(endpoint); await model().fill('error-fixture'); await password().fill(key);
    await test(); await waitHeld();
    report.busyFieldsLocked = await fieldsLocked();
    held.splice(0).forEach(reply => reply());
    const unauthorized = await errorText();
    report.unauthorizedContainsKey = unauthorized.includes(key);
    await page.screenshot({path: `${evidence}/remote-errors-401.png`, animations: 'disabled'});
    check(unauthorized.includes('HTTP 401') && !report.unauthorizedContainsKey && unauthorized.length < 400 && await settings().locator('.meta.err').getAttribute('role') === 'alert', 'HTTP 401 retains its status but masks a nonstandard credential and announces a bounded error');
    check(report.busyFieldsLocked && !(await fieldsLocked()) && await password().inputValue() === key, 'connection fields are protected while waiting and retain their draft after failure');
    check(!(await saved()).enabled, 'a failed Test does not enable or save the draft');
    await page.setViewportSize({width: 640, height: 400});
    await settings().getByRole('button', {name: 'Test connection', exact: true}).scrollIntoViewIfNeeded();
    const testBounds = await settings().getByRole('button', {name: 'Test connection', exact: true}).boundingBox();
    check(testBounds.y >= 0 && testBounds.y + testBounds.height <= 400 && await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth) && await settings().getByRole('tabpanel').evaluate(el => el.scrollWidth <= el.clientWidth), 'a long server error wraps at 640 by 400 and leaves retry reachable');
    await page.screenshot({path: `${evidence}/remote-errors-compact.png`, animations: 'disabled'});
    await page.setViewportSize({width: 1280, height: 800});
    mode = 'ok'; await test(); await settings().getByText(/^Connected:/).waitFor();
    check(!(await saved()).enabled, 'retrying a successful Test still does not enable the connection');
    await password().fill(`  ${key}  `); await test(); await settings().getByText(/^Connected:/).waitFor();
    check(requests.at(-1).keyMatches && await password().inputValue() === `  ${key}  `, 'surrounding key whitespace is trimmed for the request while preserving the unsaved draft');
    await password().fill(key);

    for (const response of ['notJson', 'missing', 'null', 'object', 'empty']) {
      mode = response; await test(); const error = await errorText();
      check(error.length > 0 && !error.includes(key) && !(await saved()).enabled, `${response} replies produce a recoverable error without leaking a credential or enabling the model`);
    }
    mode = 'echo'; await test(); await settings().getByText(/^Connected:/).waitFor();
    check(!(await settings().innerText()).includes(key), 'a successful probe cannot display an echoed authentication credential');
    mode = 'ok';
    for (const invalid of [`ftp://127.0.0.1:${port}/v1`, `http://user:password@127.0.0.1:${port}/v1`, 'not a URL']) {
      const before = requests.length;
      await url().fill(invalid); await test(); const error = await errorText();
      check(error.includes('http:// or https://') && requests.length === before, 'an invalid or credential-bearing URL is explained before any network request');
    }
    await url().fill(`${endpoint}/chat/completions`); await test(); await settings().getByText(/^Connected:/).waitFor();
    check(requests.at(-1).path === '/v1/chat/completions', 'a complete chat-completions URL is accepted without appending the path twice');
    const beforeCors = requests.length;
    await url().fill(`http://127.0.0.1:${port}/cors`); await test(); const cors = await errorText();
    check(cors.includes("Can't connect from this browser") && requests.slice(beforeCors).some(r => r.method === 'OPTIONS') && requests.slice(beforeCors).every(r => r.method !== 'POST'), 'a real rejected CORS preflight is recoverable without pretending its cause is certain');

    await url().fill(endpoint); mode = 'timeout';
    const started = Date.now(); await test();
    await settings().getByRole('button', {name: 'Testing…', exact: true}).waitFor();
    check(await fieldsLocked() && await settings().getByRole('button', {name: 'Close', exact: true}).isDisabled(), 'the timeout wait protects its connection draft and cannot be dismissed accidentally');
    await page.keyboard.press('Escape'); check(await settings().isVisible(), 'Escape does not close a pending connection test');
    const timeout = await errorText(); report.timeoutMs = Date.now() - started;
    for (let i = 0; i < 50 && !timedOutSocketClosed; i++) await page.waitForTimeout(20);
    check(timeout.includes('timed out after 30 seconds') && report.timeoutMs >= 29_000 && report.timeoutMs < 40_000 && timedOutSocketClosed, 'a stalled HTTP response body times out after 30 real seconds and closes its connection');
    check(!(await fieldsLocked()) && await url().inputValue() === endpoint && await password().inputValue() === key, 'timeout restores editing and keeps the unsaved address, model and key');
    await page.screenshot({path: `${evidence}/remote-errors-timeout.png`, animations: 'disabled'});

    mode = 'holdOk'; await settings().getByRole('button', {name: 'Save and enable', exact: true}).click(); await waitHeld();
    check(await fieldsLocked(), 'Save and enable protects the exact draft being tested');
    mode = 'ok'; held.splice(0).forEach(reply => reply());
    await settings().getByText('In use', {exact: true}).waitFor();
    check((await saved()).enabled && await password().inputValue() === '' && !(await settings().innerText()).includes(key), 'a successful save enables the tested configuration and clears the credential input');
    mode = '401'; await test(); await errorText();
    check((await saved()).enabled, 'testing a failing draft does not disable the already saved configuration');
    await model().fill('failed-save-fixture'); await settings().getByRole('button', {name: 'Save', exact: true}).click(); await errorText();
    await page.waitForFunction(() => JSON.parse(localStorage.getItem('boardcast.remote')).enabled === false);
    check((await saved()).model === 'failed-save-fixture' && await password().inputValue() === '' && await model().inputValue() === 'failed-save-fixture', 'failed Save retains the tested configuration disabled, without losing the draft');
    mode = 'ok'; await password().fill('abcdef');
    await settings().getByRole('button', {name: 'Save and enable', exact: true}).click(); await settings().getByText('In use', {exact: true}).waitFor();
    check((await password().getAttribute('placeholder')).includes('••••') && !(await settings().innerText()).includes('abcdef'), 'short credentials are fully masked in the saved-key hint');

    await settings().getByRole('button', {name: 'Close', exact: true}).click();
    mode = 'holdOk'; await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'}); await waitHeld();
    await page.getByRole('button', {name: 'Settings', exact: true}).click();
    await settings().getByRole('tab', {name: 'Remote & privacy', exact: true}).click();
    await settings().getByRole('button', {name: 'Disable', exact: true}).click();
    await page.waitForFunction(() => JSON.parse(localStorage.getItem('boardcast.remote')).enabled === false);
    await settings().getByRole('button', {name: 'Close', exact: true}).click();
    await page.locator('.chip.status').getByText('No model connected', {exact: true}).waitFor();
    const lateSuccess = page.waitForResponse(response => response.url().startsWith(endpoint) && response.request().method() === 'POST');
    mode = 'ok'; held.splice(0).forEach(reply => reply());
    await (await lateSuccess).finished();
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    check(await page.locator('.chip.status').innerText() === 'No model connected' && !(await saved()).enabled, 'a late successful startup check cannot reactivate the status of a disabled remote model');
    await page.getByRole('button', {name: 'Settings', exact: true}).click();
    await settings().getByRole('tab', {name: 'Remote & privacy', exact: true}).click();
    await model().fill('older-check'); await settings().getByRole('button', {name: 'Save and enable', exact: true}).click();
    await settings().getByText('In use', {exact: true}).waitFor(); await settings().getByRole('button', {name: 'Close', exact: true}).click();
    mode = 'holdOk'; await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'}); await waitHeld();
    await page.getByRole('button', {name: 'Settings', exact: true}).click();
    await settings().getByRole('tab', {name: 'Remote & privacy', exact: true}).click();
    await model().fill('current-check'); mode = 'ok';
    await settings().getByRole('button', {name: 'Save', exact: true}).click(); await settings().getByText('In use', {exact: true}).waitFor();
    const lateFailure = page.waitForResponse(response => response.url().startsWith(endpoint) && response.request().method() === 'POST');
    mode = '401'; held.splice(0).forEach(reply => reply()); await (await lateFailure).finished();
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    check(await settings().getByText('In use', {exact: true}).isVisible() && await page.locator('.chip.status').innerText() === 'current-check' && await settings().locator('.meta.err').count() === 0, 'a late failed check cannot replace the status of a newly saved working configuration');
    check(requests.filter(r => r.method === 'POST').every(r => r.probeOnly), 'all settings requests contain only the fixed connection probe');
    check(!report.consoleContainsKey && report.pageErrors.length === 0, 'connection failures do not expose credentials in console or cause page script errors');
    report.passed = true;
  } catch (error) {report.error = error.stack || String(error); throw error;}
  finally {
    server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
    report.requests = requests;
    await fs.writeFile(`${evidence}/remote-errors-check.json`, JSON.stringify(report, null, 2) + '\n');
  }
};
