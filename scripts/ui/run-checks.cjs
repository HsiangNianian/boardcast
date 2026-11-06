// Uses an existing Playwright installation; this does not add a project dependency.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {chromium} = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const [baseUrl = 'http://127.0.0.1:1423', ...requested] = process.argv.slice(2);
const checks = requested.length ? requested : fs.readdirSync(__dirname).filter(name => /^check-.*\.cjs$/.test(name)).sort();

(async () => {
  const browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || (fs.existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined),
    headless: true,
    args: ['--no-sandbox', ...(process.env.CHROMIUM_HOST_RULES ? [`--host-resolver-rules=${process.env.CHROMIUM_HOST_RULES}`, '--no-proxy-server'] : [])],
  });
  try {
    for (const name of checks) {
      const context = await browser.newContext({viewport: {width: 1280, height: 800}});
      const page = await context.newPage();
      try {
        const evidence = fs.mkdtempSync(path.join(os.tmpdir(), `boardcast-${path.basename(name, '.cjs')}-`));
        console.log(`${name} — evidence: ${evidence}`);
        await require(path.resolve(__dirname, name))(page, baseUrl, evidence);
        console.log(`PASS ${name}`);
      } catch (error) {
        const screenshot = `/tmp/boardcast-${path.basename(name, '.cjs')}-failure.png`;
        await page.screenshot({path: screenshot}).catch(() => {});
        console.error(`Failure screenshot: ${screenshot}`);
        throw error;
      } finally {
        await context.close();
      }
    }
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
