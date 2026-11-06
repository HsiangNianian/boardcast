// Actual browser HTTP/CORS and IndexedDB. Both named remote hosts resolve to an isolated loopback server.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const http = require('node:http');

module.exports = async function checkRemotePrivacy(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  const report = {checks: [], pageErrors: []};
  const check = (ok, label) => {assert.ok(ok, label); report.checks.push(label); console.log(`PASS: ${label}`);};
  const requests = [], healthRequests = [], held = [];
  const key = 'sk-synthetic-remote-privacy-fixture';
  let holdSummary = false, remoteFailure = false, redirectLocal = false, redirectRemote = false;
  const server = http.createServer(async (req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Headers', 'content-type,authorization');
    res.setHeader('Access-Control-Allow-Methods', 'GET,POST,OPTIONS');
    res.setHeader('Content-Type', 'application/json');
    if (req.method !== 'POST') {
      if (req.method === 'GET') healthRequests.push({host: req.headers.host.split(':')[0], url: req.url});
      res.end('{}'); return;
    }
    let text = ''; for await (const chunk of req) text += chunk;
    const body = JSON.parse(text);
    const host = req.headers.host.split(':')[0];
    const probe = body.max_tokens === 8 && body.messages.length === 1;
    const summary = !probe && String(body.messages[0]?.content).includes('helping someone');
    requests.push({host, url: req.url, probe, summary, model: body.model, keyMatches: req.headers.authorization === `Bearer ${key}`, body});
    const reply = () => {
      if ((redirectLocal && host === '127.0.0.1') || (redirectRemote && host === 'boardcast-a.test')) {
        res.statusCode = 307; res.setHeader('Location', `http://boardcast-b.test:${port}/redirect-target`); res.end('{}'); return;
      }
      if (remoteFailure && host !== '127.0.0.1' && !probe) {res.statusCode = 503; res.end(JSON.stringify({error: {message: 'Injected remote unavailable'}})); return;}
      res.end(JSON.stringify({choices: [{message: {content: probe ? 'OK' : summary ? '### Conclusions\n- Controlled meeting summary.' : 'N 「Controlled board」'}}]}));
    };
    if (holdSummary && summary && host !== '127.0.0.1') held.push(reply);
    else reply();
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  const endpoint = host => `http://${host}:${port}/v1`;
  await page.addInitScript(({port}) => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.localLlm', `http://127.0.0.1:${port}/local`);
  }, {port});
  page.on('pageerror', error => report.pageErrors.push(error.message));
  const settings = () => page.getByRole('dialog', {name: 'Settings', exact: true});
  const consent = () => page.getByRole('alertdialog', {name: 'Confirm before enabling the remote model', exact: true});
  const area = () => page.getByRole('textbox', {name: 'What was said', exact: true});
  const openSettings = async () => {
    await page.getByRole('button', {name: 'Settings', exact: true}).click();
    await settings().getByRole('tab', {name: 'Remote & privacy', exact: true}).click();
  };
  const closeSettings = () => settings().getByRole('button', {name: 'Close', exact: true}).click();
  const setLocal = async host => {
    await settings().getByRole('tab', {name: 'Board & models', exact: true}).click();
    await settings().getByPlaceholder('http://127.0.0.1:8080').fill(`http://${host}:${port}/local`);
    await settings().getByRole('tab', {name: 'Remote & privacy', exact: true}).click();
  };
  const fillRemote = async (host, model = 'fixture-model') => {
    await settings().getByPlaceholder('https://api.openai.com/v1').fill(endpoint(host));
    await settings().getByPlaceholder('e.g. gpt-4o-mini, deepseek-chat, qwen-plus').fill(model);
  };
  const current = () => page.evaluate(async () => {
    if (!(await indexedDB.databases()).some(db => db.name === 'boardcast-v2')) return null;
    const db = await new Promise((resolve, reject) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);});
    const row = await new Promise((resolve, reject) => {const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);});
    db.close(); return row;
  });
  const saved = async predicate => {
    for (let i = 0; i < 100; i++) {const m = await current(); if (m && predicate(m)) return m; await page.waitForTimeout(100);}
    throw new Error('Expected meeting state was not saved');
  };
  const submit = async text => {await area().fill(text); await area().press('Enter'); return saved(m => m.lines.some(line => line.text === text && line.ms !== undefined));};
  const retry = async () => {
    await page.getByRole('tab', {name: 'Transcript', exact: true}).click();
    await page.getByRole('button', {name: 'Retry this line', exact: true}).click();
  };
  const waitHeld = async () => {
    for (let i = 0; i < 100 && !held.length; i++) await page.waitForTimeout(50);
    assert.ok(held.length, 'expected a held summary response');
  };
  try {
    await fs.mkdir(evidence, {recursive: true});
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    report.productionScripts = await page.locator('script[src]').evaluateAll(nodes => nodes.map(node => node.src));
    for (const marker of ['PRIVATE-ONE', 'PRIVATE-TWO', 'PRIVATE-THREE']) await submit(`${marker}: ${'Synthetic meeting sentence. '.repeat(125)}`.trim());
    check(requests.every(request => request.host === '127.0.0.1'), 'before remote setup all meeting requests stay on the local endpoint');
    await openSettings(); await fillRemote('boardcast-a.test');
    await settings().locator('input[type=password]').fill(key);
    await settings().getByRole('button', {name: 'Test connection', exact: true}).click();
    await settings().getByText(/^Connected:/).waitFor();
    const probes = requests.filter(request => request.host === 'boardcast-a.test');
    check(probes.length === 1 && probes[0].probe && probes[0].body.messages[0].content === 'Reply with just the two letters OK.' && !JSON.stringify(probes).includes('PRIVATE-'), 'Test connection sends only its fixed probe over HTTP');
    check(await page.evaluate(() => !JSON.parse(localStorage.getItem('boardcast.remote') || '{}').enabled), 'successful Test does not enable the remote model');
    await settings().getByRole('button', {name: 'Save and enable', exact: true}).click(); await consent().waitFor();
    check((await consent().innerText()).includes(`boardcast-a.test:${port}`), 'consent identifies the actual target host and port');
    await consent().getByRole('button', {name: 'Not now', exact: true}).click();
    check(requests.filter(request => request.host === 'boardcast-a.test').length === 1 && await page.evaluate(() => !JSON.parse(localStorage.getItem('boardcast.remote') || '{}').enabled), 'canceling consent makes no additional request and leaves remote disabled');
    await settings().getByRole('button', {name: 'Save and enable', exact: true}).click();
    await consent().getByRole('button', {name: 'Got it, enable', exact: true}).click();
    await settings().getByText('In use', {exact: true}).waitFor();
    check(await settings().locator('input[type=password]').inputValue() === '' && !(await page.locator('body').innerText()).includes(key), 'saving clears the password input and displays only the saved-key hint');
    await closeSettings();
    for (const [width, height] of [[1280, 800], [720, 480], [640, 400]]) {
      await page.setViewportSize({width, height});
      const target = page.locator('.work-status').getByText(`Remote: boardcast-a.test:${port}`, {exact: true});
      const bounds = await target.boundingBox();
      check(bounds && bounds.y >= 0 && bounds.y + bounds.height <= height && await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `the actual remote destination remains visible at ${width} by ${height}`);
    }
    await page.screenshot({path: `${evidence}/remote-target-compact.png`, animations: 'disabled'});
    await page.setViewportSize({width: 1280, height: 800});
    await submit('PUBLIC-AFTER-CONSENT');
    check(requests.some(request => request.host === 'boardcast-a.test' && !request.probe && JSON.stringify(request.body).includes('PUBLIC-AFTER-CONSENT')), 'accepted consent enables actual meeting requests to that host');

    await openSettings(); await fillRemote('boardcast-a.test', 'fixture-next');
    const beforeKeep = requests.length;
    await settings().getByRole('button', {name: 'Save', exact: true}).click(); await settings().getByText('In use', {exact: true}).waitFor();
    await settings().getByText(/^Enabled:/).waitFor();
    check(requests.slice(beforeKeep).some(request => request.probe && request.model === 'fixture-next' && request.keyMatches), 'a blank key keeps the saved credential when changing the model');
    await fillRemote('boardcast-b.test', 'fixture-next'); await settings().getByRole('button', {name: 'Save', exact: true}).click(); await consent().waitFor();
    check((await consent().innerText()).includes(`boardcast-b.test:${port}`) && requests.every(request => request.host !== 'boardcast-b.test'), 'a different host requires new consent before any enable request');
    await consent().getByRole('button', {name: 'Not now', exact: true}).click();
    await fillRemote('boardcast-a.test', 'fixture-next'); await closeSettings();

    holdSummary = true;
    const beforeSummary = requests.length;
    await page.getByRole('button', {name: 'Summary', exact: true}).click(); await waitHeld();
    await openSettings(); await settings().getByRole('checkbox', {name: 'Local model only for this meeting', exact: true}).check(); await closeSettings();
    await saved(m => m.localOnly);
    check(await page.locator('.work-status').getByText('Local only', {exact: true}).isVisible() && !(await page.locator('.work-status').innerText()).includes('boardcast-a.test'), 'the visible destination changes to the local-only meeting policy');
    const whenPrivate = requests.length;
    holdSummary = false; held.splice(0).forEach(reply => reply());
    await saved(m => m.summary?.includes('Controlled meeting summary'));
    report.summaryRequests = requests.slice(beforeSummary).map(({host, probe, summary}) => ({host, probe, summary}));
    check(requests.slice(whenPrivate).every(request => request.host === '127.0.0.1'), 'switching to local-only during a multipart summary blocks every subsequent remote request');
    check(await page.locator('.toast.err').count() === 0, 'a summary completed locally is not presented as a failed operation');
    await submit('PRIVATE-AFTER-TOGGLE');
    check(requests.slice(whenPrivate).every(request => request.host === '127.0.0.1'), 'new board input also stays local after the meeting privacy switch');
    await page.screenshot({path: `${evidence}/remote-local-only.png`, animations: 'disabled'});

    await openSettings(); await settings().getByRole('checkbox', {name: 'Local model only for this meeting', exact: true}).uncheck(); await closeSettings();
    remoteFailure = true;
    const beforeFallback = requests.length;
    const fallback = await submit('FALLBACK-WHEN-REMOTE-FAILS');
    check(requests.slice(beforeFallback).some(request => request.host === 'boardcast-a.test') && requests.slice(beforeFallback).some(request => request.host === '127.0.0.1') && fallback.lines.at(-1).fellBack, 'a real HTTP 503 falls back locally and records that outcome on the line');
    remoteFailure = false; holdSummary = true;
    await page.getByRole('button', {name: 'Summarize again', exact: true}).click(); await waitHeld();
    await openSettings(); await settings().getByRole('button', {name: 'Disable', exact: true}).click();
    await page.waitForFunction(() => !JSON.parse(localStorage.getItem('boardcast.remote')).enabled); await closeSettings();
    const whenDisabled = requests.length;
    holdSummary = false; held.splice(0).forEach(reply => reply());
    await page.getByRole('button', {name: 'Keep current summary', exact: true}).waitFor();
    check(requests.slice(whenDisabled).every(request => request.host === '127.0.0.1'), 'disabling the remote model also stops later parts of an existing summary from using it');
    await page.getByRole('button', {name: 'Keep current summary', exact: true}).click();
    await page.getByRole('tab', {name: 'Transcript', exact: true}).click();
    const remoteLine = page.locator('article.line').filter({hasText: 'PUBLIC-AFTER-CONSENT'});
    const fallbackLine = page.locator('article.line').filter({hasText: 'FALLBACK-WHEN-REMOTE-FAILS'});
    check((await remoteLine.locator('summary').innerText()).includes('Remote fixture-model'), 'disabling remote preserves the original model source on a completed line');
    check((await fallbackLine.locator('summary').innerText()).includes('Local') && await fallbackLine.locator('em').count() > 0, 'a completed fallback line retains its local source and fallback marker');
    await page.screenshot({path: `${evidence}/remote-disabled-history.png`, animations: 'disabled'});
    await page.getByRole('tab', {name: 'Summary', exact: true}).click();

    await openSettings(); await settings().getByRole('button', {name: 'Save and enable', exact: true}).click();
    await settings().getByText('In use', {exact: true}).waitFor(); await closeSettings();
    holdSummary = true;
    const oldSummary = (await current()).summary;
    await page.getByRole('button', {name: 'Summarize again', exact: true}).click(); await waitHeld();
    const whenCanceled = requests.length;
    await page.getByRole('button', {name: 'Stop', exact: true}).click();
    await page.getByRole('button', {name: 'Stop', exact: true}).waitFor({state: 'hidden'});
    holdSummary = false; held.splice(0).forEach(reply => reply());
    check(requests.length === whenCanceled && (await current()).summary === oldSummary && await page.getByRole('button', {name: 'Keep current summary', exact: true}).count() === 0, 'canceling a multipart summary aborts browser transport without later requests or replacing the saved summary');

    await openSettings(); await settings().getByRole('button', {name: 'Disable', exact: true}).click();
    await page.waitForFunction(() => !JSON.parse(localStorage.getItem('boardcast.remote')).enabled);
    await setLocal('boardcast-b.test'); await closeSettings();
    const beforeNonlocal = requests.length;
    await area().fill('PRIVATE-NONLOCAL-ADDRESS'); await area().press('Enter');
    const rejected = await saved(m => m.lines.at(-1)?.text === 'PRIVATE-NONLOCAL-ADDRESS' && (m.lines.at(-1).failed || m.lines.at(-1).ms !== undefined));
    check(rejected.lines.at(-1).failed && requests.length === beforeNonlocal, 'an external address in the local-model field cannot receive meeting content without remote consent');
    await openSettings(); await settings().getByRole('tab', {name: 'Board & models', exact: true}).click();
    check(await settings().getByText('Use a model on this device here. For another computer, use Remote & privacy.', {exact: true}).isVisible(), 'a nonlocal model address has an actionable explanation');
    await page.screenshot({path: `${evidence}/local-address-guidance.png`, animations: 'disabled'});
    await setLocal('127.0.0.1'); await closeSettings();
    await retry();
    const recovered = await saved(m => m.lines.at(-1)?.text === 'PRIVATE-NONLOCAL-ADDRESS' && !m.lines.at(-1).failed && m.lines.at(-1).ms !== undefined);
    check(recovered.lines.filter(line => line.text === 'PRIVATE-NONLOCAL-ADDRESS').length === 1 && requests.at(-1).host === '127.0.0.1', 'fixing the local address lets the original saved line retry without a duplicate');

    await openSettings(); await settings().getByRole('button', {name: 'Save and enable', exact: true}).click();
    await settings().getByText('In use', {exact: true}).waitFor();
    await setLocal('127.boardcast.test'); await closeSettings();
    holdSummary = true;
    await page.getByRole('button', {name: 'Summarize again', exact: true}).click(); await waitHeld();
    await openSettings(); await settings().getByRole('checkbox', {name: 'Local model only for this meeting', exact: true}).check(); await closeSettings();
    const beforePrivateFallback = requests.length;
    holdSummary = false; held.splice(0).forEach(reply => reply());
    await page.locator('.toast.err').filter({hasText: 'Use a model on this device here.'}).waitFor();
    check(requests.length === beforePrivateFallback && (await current()).summary === oldSummary, 'a captured remote summary cannot fall back to a 127-prefixed domain after switching to local-only');
    await area().fill('PRIVATE-LOOKALIKE-LOCAL'); await area().press('Enter');
    await saved(m => m.lines.at(-1)?.text === 'PRIVATE-LOOKALIKE-LOCAL' && m.lines.at(-1).failed);
    check(requests.length === beforePrivateFallback, 'new input also refuses a 127-prefixed domain while local-only is active');
    await openSettings(); await setLocal('127.0.0.1'); await closeSettings();
    await retry();
    await saved(m => m.lines.at(-1)?.text === 'PRIVATE-LOOKALIKE-LOCAL' && !m.lines.at(-1).failed && m.lines.at(-1).ms !== undefined);
    redirectLocal = true;
    const beforeLocalRedirect = requests.length;
    await area().fill('PRIVATE-LOCAL-REDIRECT'); await area().press('Enter');
    await saved(m => m.lines.at(-1)?.text === 'PRIVATE-LOCAL-REDIRECT' && m.lines.at(-1).failed);
    check(requests.slice(beforeLocalRedirect).length === 1 && requests.at(-1).host === '127.0.0.1', 'a local HTTP 307 cannot forward meeting content to another host');
    redirectLocal = false;
    await retry();
    await saved(m => m.lines.at(-1)?.text === 'PRIVATE-LOCAL-REDIRECT' && !m.lines.at(-1).failed && m.lines.at(-1).ms !== undefined);

    await openSettings(); await settings().getByRole('checkbox', {name: 'Local model only for this meeting', exact: true}).uncheck();
    redirectRemote = true;
    const beforeRemoteRedirect = requests.length;
    await settings().getByRole('button', {name: 'Test connection', exact: true}).click();
    await settings().locator('.meta.err').waitFor();
    check(requests.slice(beforeRemoteRedirect).length === 1 && requests.at(-1).host === 'boardcast-a.test', 'a remote connection probe does not follow a redirect to a different host');
    await closeSettings();
    const beforeRemoteLine = requests.length;
    const redirected = await submit('PUBLIC-REMOTE-REDIRECT');
    check(redirected.lines.at(-1).fellBack && requests.slice(beforeRemoteLine).map(request => request.host).join(',') === 'boardcast-a.test,127.0.0.1', 'redirected remote meeting requests fall back locally without contacting the unapproved target');
    redirectRemote = false;
    check(healthRequests.length > 0 && healthRequests.every(request => request.host === '127.0.0.1'), 'local health checks also stay on this device');
    check(report.pageErrors.length === 0, 'remote privacy flow has no browser script errors');
    report.passed = true;
  } catch (error) {report.error = error.stack || String(error); throw error;}
  finally {
    report.requests = requests.map(({body, ...request}) => ({...request, containsMeeting: JSON.stringify(body).includes('PRIVATE-') || JSON.stringify(body).includes('PUBLIC-') || JSON.stringify(body).includes('FALLBACK-')}));
    report.healthRequests = healthRequests;
    server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
    await fs.writeFile(`${evidence}/remote-privacy-check.json`, JSON.stringify(report, null, 2) + '\n');
  }
};
