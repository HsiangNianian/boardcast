// Resolve a visible meeting action through the compact More menu when necessary.
async function meetingAction(page, name) {
  const direct = page.locator('.meeting-actions').getByRole('button', {name, exact:true});
  if (!await page.evaluate(() => matchMedia('(max-width: 900px)').matches)) {
    await direct.waitFor({state:'visible'});
    return direct;
  }
  const item = page.locator('.meeting-menu').getByRole('menuitem', {name, exact:true});
  if (!await item.isVisible()) {
    const more = page.locator('.meeting-more');
    if (await page.evaluate(() => matchMedia('(pointer: coarse)').matches)) await more.tap();
    else await more.click();
  }
  await item.waitFor({state:'visible'});
  return item;
}
module.exports = meetingAction;
module.exports.click = async (page, name, options) => (await meetingAction(page, name)).click(options);
