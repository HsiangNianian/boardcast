const assert = require('node:assert/strict');

// Two realms share real IndexedDB. Only an image hash is gated to order the race.
module.exports = async (page, baseUrl) => {
  const other = await page.context().newPage();
  const open = async target => {
    await target.route('**/storage-delete-check', route => route.fulfill({contentType: 'text/html', body: '<!doctype html><title>Storage deletion check</title>'}));
    await target.goto(`${baseUrl}/storage-delete-check`);
    await target.evaluate(async () => {
      window.store = (await import('/src/meetings.ts')).meetings;
      window.database = (legacy, mode, run) => new Promise((resolve, reject) => {
        const request = indexedDB.open(legacy ? 'boardcast' : 'boardcast-v2', legacy ? 2 : 1);
        request.onupgradeneeded = () => {
          request.result.createObjectStore('meetings', {keyPath: 'id'});
          request.result.createObjectStore('assets');
        };
        request.onerror = () => reject(request.error);
        request.onsuccess = () => {
          const db = request.result, tx = db.transaction(['meetings', 'assets'], mode);
          const result = run(tx);
          tx.oncomplete = () => { db.close(); resolve(result?.result); };
          tx.onabort = tx.onerror = () => { db.close(); reject(tx.error); };
        };
      });
    });
  };
  const results = [];
  try {
    await open(page);
    await open(other);
    for (const version of ['revision-1', 'revision-0', 'legacy']) {
      for (const duringImage of [false, true]) {
        const id = `${version}-${duringImage ? 'during-image' : 'after-delete'}`;
        await page.evaluate(async ({id, version}) => {
          const graph = {nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1};
          const record = {version: version === 'legacy' ? 1 : 2, id, title: id, createdAt: 1, updatedAt: 1, graph, history: [], lines: [],
            ...(version === 'revision-1' ? {storageRevision: 1} : {})};
          await window.database(version === 'legacy', 'readwrite', tx => tx.objectStore('meetings').put(record));
          window.draft = await window.store.load(id);
          window.draft.title = 'Retain this unsaved edit';
        }, {id, version});
        if (duringImage) {
          await page.evaluate(() => {
            const nativeDigest = crypto.subtle.digest.bind(crypto.subtle);
            const gate = new Promise(resolve => { window.releaseImage = resolve; });
            window.imageEntered = false;
            crypto.subtle.digest = async (algorithm, data) => {
              if (new TextDecoder().decode(data).startsWith('data:image/')) {
                window.imageEntered = true;
                await gate;
              }
              return nativeDigest(algorithm, data);
            };
            window.restoreDigest = () => { crypto.subtle.digest = nativeDigest; };
            window.draft.pages = [{graph: window.draft.graph, underlay: 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="red"/></svg>')}];
            window.saving = window.store.save(window.draft).then(() => null, error => String(error));
          });
          await page.waitForFunction(() => window.imageEntered);
        }
        await other.evaluate(id => window.store.remove(id), id);
        const error = await page.evaluate(async duringImage => {
          if (!duringImage) return window.store.save(window.draft).then(() => null, error => String(error));
          window.releaseImage();
          try { return await window.saving; } finally { window.restoreDigest(); }
        }, duringImage);
        const state = await other.evaluate(async id => ({
          loaded: await window.store.load(id),
          raw: await window.store.raw(id),
          listed: (await window.store.list()).some(m => m.id === id),
          assets: await window.database(false, 'readonly', tx => tx.objectStore('assets').getAllKeys(IDBKeyRange.bound(`${id}/`, `${id}/\uffff`))),
        }), id);
        results.push({id, rejected: !!error, absent: !state.loaded && !state.raw && !state.listed, assets: state.assets.length, error});
      }
    }
    console.log(JSON.stringify(results, null, 2));
    for (const result of results) {
      assert.equal(result.rejected, true, `${result.id}: stale save must fail`);
      assert.equal(result.absent, true, `${result.id}: meeting must stay deleted`);
      assert.equal(result.assets, 0, `${result.id}: late image write must not recreate deleted data`);
    }
    await open(page);
    const persisted = await page.evaluate(async () => {
      const graph = {nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1};
      const stale = {version: 2, id: 'revision-0-after-delete', title: 'Stale after reload', createdAt: 1, updatedAt: 1, graph, history: [], lines: []};
      const error = await window.store.save(stale).then(() => null, error => String(error));
      const original = {...stale, version: 1, id: 'migration-control', title: 'Preserve original', pages: [{graph, underlay: 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"/>')}]};
      await window.database(true, 'readwrite', tx => tx.objectStore('meetings').put(original));
      const loaded = await window.store.load(original.id);
      await window.store.save({...loaded, title: 'Saved migration'});
      const migrated = await window.store.load(original.id);
      const legacy = await window.database(true, 'readonly', tx => tx.objectStore('meetings').get(original.id));
      return {error, migrated, original, legacy};
    });
    assert.match(persisted.error, /deleted/);
    assert.equal(persisted.migrated.storageRevision, 1);
    assert.equal(persisted.migrated.title, 'Saved migration');
    assert.equal(persisted.migrated.pages[0].underlay, persisted.original.pages[0].underlay);
    assert.deepEqual(persisted.legacy, persisted.original, 'ordinary migration keeps original bytes');

    await other.evaluate(async () => { window.draft = await window.store.load('migration-control'); });
    await page.evaluate(async () => { await window.store.save({...await window.store.load('migration-control'), title: 'Newer edit'}); });
    const conflict = await other.evaluate(() => window.store.save({...window.draft, title: 'Stale edit'}).then(() => null, String));
    assert.match(conflict, /another window/);
    assert.equal(await page.evaluate(async () => (await window.store.load('migration-control')).title), 'Newer edit');

    const copy = await page.evaluate(async () => {
      const old = await window.database(true, 'readonly', tx => tx.objectStore('meetings').get('migration-control'));
      await window.database(true, 'readwrite', tx => tx.objectStore('meetings').put({...old, title: 'Edited in older version'}));
      const listed = (await window.store.list()).some(m => m.id === 'legacy:migration-control');
      const recovered = await window.store.load('legacy:migration-control');
      await window.store.remove('legacy:migration-control');
      await window.store.save(recovered);
      return {listed, recovered: await window.store.load(recovered.id), current: await window.store.load('migration-control'), old: await window.store.raw('legacy:migration-control')};
    });
    assert.equal(copy.listed, true);
    assert.equal(copy.old, null);
    assert.equal(copy.current.title, 'Newer edit');
    assert.equal(copy.recovered.title.startsWith('Edited in older version'), true);
    assert.notEqual(copy.recovered.id, 'migration-control');

    const aborted = await page.evaluate(async () => {
      const id = 'delete-abort-control';
      const base = await window.store.load('migration-control');
      await window.store.save({...base, id, migration: undefined, storageRevision: undefined});
      const before = await window.store.raw(id);
      const images = () => window.database(false, 'readonly', tx => tx.objectStore('assets').getAll(IDBKeyRange.bound(`${id}/`, `${id}/\uffff`)));
      const originalImages = await images();
      const put = IDBObjectStore.prototype.put;
      IDBObjectStore.prototype.put = function(value, key) {
        const request = put.apply(this, arguments);
        if (value === true && key === `deleted:${id}`) request.addEventListener('success', () => this.transaction.abort(), {once: true});
        return request;
      };
      let error;
      try { error = await window.store.remove(id).then(() => null, String); }
      finally { IDBObjectStore.prototype.put = put; }
      const retained = await window.store.raw(id), retainedImages = await images();
      const marker = await window.database(false, 'readonly', tx => tx.objectStore('assets').get(`deleted:${id}`));
      await window.store.save({...await window.store.load(id), title: 'Retry after failed delete'});
      await window.store.remove(id);
      const currentRecords = await window.database(false, 'readonly', tx => tx.objectStore('meetings').getAll());
      const legacyRecords = await window.database(true, 'readonly', tx => tx.objectStore('meetings').getAll());
      return {error, before, retained, originalImages, retainedImages, marker, deleted: await window.store.load(id),
        allRecordsValid: [...currentRecords, ...legacyRecords].every(m => typeof m.title === 'string' && Array.isArray(m.lines) && m.graph)};
    });
    assert.ok(aborted.error, 'a transaction abort after request success must reject deletion');
    assert.deepEqual(aborted.retained, aborted.before);
    assert.deepEqual(aborted.retainedImages, aborted.originalImages);
    assert.equal(aborted.originalImages.length, 1);
    assert.equal(aborted.marker, undefined, 'aborted deletion must not persist a marker');
    assert.equal(aborted.deleted, null);
    assert.equal(aborted.allRecordsValid, true, 'markers must not become fake records in older clients');
    console.log('PASS reload persistence, migration/images, stale revision, older-copy recovery and delete-abort/retry controls');
  } finally {
    await other.close();
  }
};
