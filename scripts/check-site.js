// Playwright browser_run_code_unsafe: pass this file as filename, with the site on port 1430.
async (page) => {
  const assert = (ok, message) => { if (!ok) throw new Error(message); };
  const base = 'http://127.0.0.1:1430';
  const errors = [];
  const onError = error => errors.push(error.message);
  const onResponse = response => {
    if (response.url().startsWith(base) && response.status() >= 400) errors.push(`${response.status()} ${response.url()}`);
  };
  page.on('pageerror', onError);
  page.on('response', onResponse);
  const noOverflow = target => target.waitForFunction(() => document.documentElement.scrollWidth <= innerWidth, null, { timeout: 5000 });
  const ink = async target => target.waitForFunction(() => document.querySelector('#board path[d]:not([d=""])'));
  const inkFits = async target => assert(await target.evaluate(() => {
    const board = document.querySelector('#board');
    const view = board.viewBox.baseVal, ink = board.getBBox();
    return ink.width > 0 && ink.height > 0 && ink.x >= view.x - 1 && ink.y >= view.y - 1
      && ink.x + ink.width <= view.x + view.width + 1 && ink.y + ink.height <= view.y + view.height + 1;
  }), 'All drawn ink must fit inside the canvas');
  try {
    await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: 'dark' });
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.goto(base);
    await page.evaluate(() => sessionStorage.removeItem('boardcast-theme'));
    await page.goto(`${base}/demo/?theme=light&check=1#board`);
    const originalURL = page.url();
    await ink(page);
    assert(await page.locator('#samples button').count() === 5, 'Five samples must load');
    await page.locator('#listen').click();
    assert(await page.locator('#listen-label').textContent() === 'resume', 'Pausing must show the resume action');
    await page.waitForTimeout(1700);
    await page.evaluate(() => { window.checkedInk = document.querySelector('#board path[d]'); });
    for (const [theme, color] of [['dark', 'rgb(236, 236, 230)'], ['light', 'rgb(30, 30, 30)'], ['dark', 'rgb(236, 236, 230)']]) {
      await page.emulateMedia({ colorScheme: theme });
      await page.waitForFunction(color => getComputedStyle(window.checkedInk).fill === color, color);
      assert(await page.evaluate(color => window.checkedInk.isConnected && getComputedStyle(window.checkedInk).fill === color, color), `${theme}: existing ink must recolor without a restart`);
      assert(page.url() === originalURL, 'System theme changes must not rewrite the URL');
    }
    for (let i = 0; i < 5; i++) {
      await page.locator('#samples button').nth(i).click();
      await page.waitForFunction(i => document.querySelectorAll('#samples button')[i].classList.contains('on') && document.querySelector('#listen-label').textContent === 'pause', i);
      await ink(page);
      await inkFits(page);
    }
    await page.locator('#replay').click();
    await ink(page);

    for (const route of ['/', '/demo/']) {
      for (const theme of ['light', 'dark']) {
        await page.emulateMedia({ colorScheme: theme });
        await page.setViewportSize({ width: 1440, height: 1000 });
        await page.goto(`${base}${route}`);
        assert(await page.locator('[data-theme-value], .appearance-bar, nav #theme-toggle').count() === 0, 'The old header theme picker must stay removed');
        const bg = theme === 'light' ? 'rgb(255, 255, 255)' : 'rgb(0, 0, 0)';
        assert(await page.evaluate(bg => getComputedStyle(document.body).backgroundColor === bg, bg), 'Initial appearance must follow the system');
        await noOverflow(page);
        for (const width of [320, 390, 414, 768, 820]) {
          await page.setViewportSize({ width, height: 844 });
          await page.waitForTimeout(400);
          await noOverflow(page);
        }
      }
    }

    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.goto(`${base}/`);
    await page.waitForFunction(() => document.querySelectorAll('#ruler-left span').length === 8);
    const rulerPositions = () => page.locator('.ruler').evaluateAll(nodes => nodes.map(el => {
      const r = el.getBoundingClientRect();
      return [r.x, r.y, r.width, r.height];
    }));
    const rulersAtTop = await rulerPositions();
    await page.evaluate(() => scrollTo(0, 650));
    await page.waitForFunction(() => Number(document.querySelector('#ruler-left span').textContent) > 1);
    assert(JSON.stringify(await rulerPositions()) === JSON.stringify(rulersAtTop), 'Rulers must stay fixed while the content scrolls');
    assert(await page.locator('#ruler-top').textContent() === 'ABCDEFGHIJKL', 'The top ruler letters must stay fixed');
    const numbers = await page.locator('#ruler-left span').allTextContents();
    assert(numbers.every((n, i) => Number(n) === Number(numbers[0]) + i), 'Scrolling ruler numbers must remain consecutive');
    await page.setViewportSize({ width: 1440, height: 700 });
    await page.waitForFunction(first => Number(document.querySelector('#ruler-left span').textContent) > first, Number(numbers[0]));
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.evaluate(() => scrollTo(0, 0));
    await page.waitForFunction(() => document.querySelector('#ruler-left span').textContent === '1');
    await page.waitForFunction(() => [...document.querySelectorAll('#ink > g')].some(g => g.children.length === 3 && [...g.children].every(p => p.matches('path.note[d]'))));
    assert(await page.evaluate(() => {
      const arrow = [...document.querySelectorAll('#ink > g')].find(g => g.children.length === 3 && [...g.children].every(p => p.matches('path.note[d]'))).firstElementChild;
      const start = arrow.getPointAtLength(0), end = arrow.getPointAtLength(arrow.getTotalLength());
      return end.y < start.y && end.x < start.x;
    }), 'The note arrow must point up toward the headline');
    assert(await page.locator('iframe').count() === 1 && await page.locator('#hero #live iframe').count() === 1, 'The real demo must appear once, in the hero');
    assert(await page.locator('.try iframe').evaluate(el => el.getBoundingClientRect().bottom <= innerHeight), 'The desktop first fold must show the whole demo');
    assert(await page.locator('.titleblock').evaluate(el => getComputedStyle(el).visibility === 'hidden'), 'Sheet information must not cover the hero demo');
    const word = await page.locator('#hero-writing [data-word="talk"]').boundingBox();
    const cx = word.x + word.width / 2, cy = word.y + word.height / 2;
    await page.mouse.move(cx + word.width / 2 + 6, cy);
    await page.mouse.down();
    for (let i = 1; i <= 32; i++) {
      const angle = 2 * Math.PI * i / 32;
      await page.mouse.move(cx + (word.width / 2 + 6) * Math.cos(angle), cy + (word.height / 2 + 6) * Math.sin(angle));
    }
    await page.mouse.up();
    await page.waitForFunction(() => document.querySelector('#toast').textContent.includes('circled “talk”'));
    await page.waitForFunction(() => getComputedStyle(document.querySelector('.titleblock')).visibility === 'visible');
    await page.evaluate(() => scrollTo(0, 0));
    await page.waitForFunction(() => getComputedStyle(document.querySelector('.titleblock')).visibility === 'hidden');
    await page.locator('#again').click();
    await page.locator('#hero-writing [data-word="talk"]').waitFor();
    await page.locator('.try iframe').scrollIntoViewIfNeeded();
    await page.waitForFunction(() => document.querySelector('iframe').contentDocument?.querySelector('#board path[d]'));
    const frame = page.frames().find(f => f.url().includes('/demo/'));
    assert(await frame.evaluate(() => {
      const a = document.querySelector('.side').getBoundingClientRect(), b = document.querySelector('.canvas').getBoundingClientRect();
      return a.width > 0 && a.right <= b.left + 1 && a.top === b.top;
    }), 'Desktop transcript must sit to the left of the board');
    assert(await frame.locator('.dock button:visible').count() === 2, 'Embedded demo must only show working playback controls');
    await frame.locator('#listen').click();
    await page.waitForTimeout(1700);
    await frame.evaluate(() => { window.checkedInk = document.querySelector('#board path[d]'); });
    await page.emulateMedia({ colorScheme: 'light' });
    await frame.waitForFunction(() => getComputedStyle(window.checkedInk).fill === 'rgb(30, 30, 30)');
    assert(await frame.evaluate(() => window.checkedInk.isConnected), 'System theme change must preserve embedded playback');
    assert(!new URL(frame.url()).searchParams.has('theme'), 'Embedded appearance must not depend on a URL override');
    await inkFits(frame);
    for (const width of [320, 390, 414, 700, 768, 820]) {
      await page.setViewportSize({ width, height: 844 });
      await page.waitForTimeout(400);
      await noOverflow(page);
      await noOverflow(frame);
      assert(await page.locator('#live').evaluate(el => el.getBoundingClientRect().width >= innerWidth - (innerWidth <= 600 ? 32 : 80)), 'Narrow layouts must use the width available outside the rulers');
      if (width > 600) assert(await page.locator('#live').evaluate(el => el.getBoundingClientRect().left >= document.querySelector('#ruler-left').getBoundingClientRect().right), 'The fixed left ruler must not cover the demo');
      assert(await frame.locator('#board').evaluate(el => el.getBoundingClientRect().height >= 270), 'The board must keep enough height after the transcript stacks');
      if (width <= 700) assert(await frame.evaluate(() => {
        const a = document.querySelector('.side').getBoundingClientRect(), b = document.querySelector('.canvas').getBoundingClientRect();
        return a.height >= 140 && a.bottom <= b.top + 1 && Math.abs(a.width - b.width) < 1;
      }), 'Phones must show the transcript above the board');
      assert(await frame.locator('.samples button').first().evaluate(el => el.getBoundingClientRect().height >= 44), 'Samples must have a 44px touch target');
      assert(await frame.locator('#replay').evaluate(el => el.getBoundingClientRect().height >= 44), 'Playback controls must have a 44px touch target');
    }
    await page.setViewportSize({ width: 320, height: 844 });
    await frame.locator('#samples button').last().click();
    await frame.waitForFunction(() => {
      const last = document.querySelector('#samples button:last-child');
      return last.classList.contains('on') && last.getBoundingClientRect().right <= innerWidth + 1;
    });
    await frame.locator('#replay').click();
    await ink(frame);
    await inkFits(frame);
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.locator('#after').scrollIntoViewIfNeeded();
    const dots = page.locator('.dot-card canvas').first();
    await page.waitForFunction(() => document.querySelector('.dot-card canvas').getContext('2d').fillStyle === '#252820');
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForFunction(() => document.querySelector('.dot-card canvas').getContext('2d').fillStyle === '#f2f2f2');
    assert(await dots.evaluate(canvas => canvas.getContext('2d').fillStyle === '#f2f2f2'), 'Canvas icons must recolor even with reduced motion');

    for (const route of ['faq', 'privacy', 'terms']) {
      await page.emulateMedia({ colorScheme: 'dark' });
      await page.goto(`${base}/${route}/`);
      assert(await page.locator('footer > span').textContent() === '2026-11-10', 'Document footers must show just the update date');
      assert(await page.evaluate(() => getComputedStyle(document.body).backgroundColor === 'rgb(0, 0, 0)'), 'Documents must initially follow the system theme');
      await page.emulateMedia({ colorScheme: 'light' });
      await page.waitForFunction(() => getComputedStyle(document.body).backgroundColor === 'rgb(255, 255, 255)');
      await page.waitForFunction(() => document.querySelector('meta[name="theme-color"]').content === '#fff');
      assert(await page.locator('header .brand').evaluate(link => !new URL(link.href).searchParams.has('theme')), 'Links must not carry theme overrides');
    }
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto(`${base}/`);
    await page.waitForFunction(() => document.querySelector('iframe').contentDocument?.querySelector('#board path[d]'));
    const embedded = page.frames().find(f => f.url().includes('/demo/'));
    await embedded.locator('#listen').click();
    await page.waitForTimeout(1700);
    await embedded.evaluate(() => { window.checkedInk = document.querySelector('#board path[d]'); });
    await page.locator('#after').scrollIntoViewIfNeeded();
    const toggle = page.locator('#theme-toggle');
    assert(await toggle.count() === 1 && await toggle.getAttribute('aria-label') === 'Switch to light theme', 'One accessible theme button belongs in the sheet information');
    await toggle.waitFor({ state: 'visible' });
    await toggle.focus();
    await page.keyboard.press('Enter');
    await embedded.waitForFunction(() => getComputedStyle(window.checkedInk).fill === 'rgb(30, 30, 30)');
    assert(await embedded.evaluate(() => window.checkedInk.isConnected && document.querySelector('#listen-label').textContent === 'resume'), 'Manual themes must preserve embedded ink and the paused state');
    assert(await page.evaluate(() => document.documentElement.dataset.theme === 'light' && sessionStorage.getItem('boardcast-theme') === 'light'), 'Manual light theme must persist for this tab');
    await page.waitForFunction(() => document.querySelector('.dot-card canvas').getContext('2d').fillStyle === '#252820');
    await page.emulateMedia({ colorScheme: 'light' });
    await page.emulateMedia({ colorScheme: 'dark' });
    assert(await page.evaluate(() => getComputedStyle(document.body).backgroundColor === 'rgb(255, 255, 255)'), 'A manual theme must survive system changes');
    await page.reload();
    assert(await page.evaluate(() => getComputedStyle(document.body).backgroundColor === 'rgb(255, 255, 255)'), 'A refresh must retain the chosen theme');
    for (const route of ['faq', 'demo']) {
      await page.goto(`${base}/${route}/`);
      assert(await page.evaluate(() => getComputedStyle(document.body).backgroundColor === 'rgb(255, 255, 255)'), 'Documents and the full demo must share the tab theme');
    }
    await page.goto(`${base}/`);
    await page.locator('#signals').scrollIntoViewIfNeeded();
    await page.locator('#theme-toggle').click();
    assert(await page.evaluate(() => !document.documentElement.hasAttribute('data-theme') && !sessionStorage.getItem('boardcast-theme')), 'Switching back to the system theme must restore automatic detection');
    await page.emulateMedia({ colorScheme: 'light' });
    await page.waitForFunction(() => getComputedStyle(document.body).backgroundColor === 'rgb(255, 255, 255)');
    await page.locator('#theme-toggle').click();
    await page.waitForFunction(() => getComputedStyle(document.body).backgroundColor === 'rgb(0, 0, 0)');
    await page.waitForFunction(() => document.querySelector('iframe').contentDocument?.documentElement.dataset.theme === 'dark');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.locator('#signals').scrollIntoViewIfNeeded();
    assert(await page.locator('#theme-toggle').evaluate(el => el.getBoundingClientRect().width >= 44 && el.getBoundingClientRect().height >= 44), 'The phone theme button must have a 44px touch target');
    await page.locator('#theme-toggle').click();
    await page.waitForFunction(() => getComputedStyle(document.body).backgroundColor === 'rgb(255, 255, 255)');
    await noOverflow(page);
    await page.evaluate(() => sessionStorage.setItem('boardcast-theme', 'paper'));
    await page.reload();
    assert(await page.evaluate(() => !document.documentElement.hasAttribute('data-theme')), 'Unsupported saved themes must fall back to the system');
    await page.evaluate(() => sessionStorage.removeItem('boardcast-theme'));
    const privateContext = await page.context().browser().newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, colorScheme: 'dark', reducedMotion: 'reduce' });
    try {
      await privateContext.addInitScript(() => Object.defineProperty(window, 'sessionStorage', { get() { throw new Error('Storage blocked'); } }));
      const phone = await privateContext.newPage();
      await phone.goto(`${base}/`);
      await phone.waitForFunction(() => document.querySelector('iframe').contentDocument?.querySelector('#board path[d]'));
      await phone.locator('#signals').scrollIntoViewIfNeeded();
      await phone.locator('#theme-toggle').tap();
      await phone.waitForFunction(() => getComputedStyle(document.body).backgroundColor === 'rgb(255, 255, 255)' && getComputedStyle(document.querySelector('iframe').contentDocument.body).backgroundColor === 'rgb(255, 255, 255)');
      await phone.locator('#theme-toggle').tap();
      await phone.waitForFunction(() => getComputedStyle(document.body).backgroundColor === 'rgb(0, 0, 0)' && getComputedStyle(document.querySelector('iframe').contentDocument.body).backgroundColor === 'rgb(0, 0, 0)');
    } finally { await privateContext.close(); }
    assert(errors.length === 0, errors.join('\n'));
    return { passed: ['system theme changes preserve ink', 'old theme URLs cannot override the system', 'five samples, playback controls and canvas framing', 'automatic Light and Dark on desktop and mobile', 'fixed HUD rulers with scrolling numbers', 'hero arrow, circle targets and rewrite', 'real demo in the first fold', 'phone and tablet demo sizing and touch targets', 'embedded demo follows the system', 'canvas colors with reduced motion', 'document and browser theme colors', 'keyboard theme toggle preserves playback', 'tab preference survives reload and navigation', 'return to automatic theme and reject unsupported themes', 'touch theme switching with storage blocked'], errors };
  } finally {
    page.off('pageerror', onError);
    page.off('response', onResponse);
    await page.emulateMedia({ reducedMotion: null, colorScheme: null });
  }
}
