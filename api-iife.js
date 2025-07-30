if ('__TAURI__' in window) {
    var __TAURI_PLUGIN_RUSQLITE__ = (function () {
        'use strict';
        async function e(e, t = {}, s) {
            return window.__TAURI_INTERNALS__.invoke(e, t, s);
        }
        'function' == typeof SuppressedError && SuppressedError;
        class t {
            constructor(e) {
                this.path = e;
            }
            static async load(s) {
                const n = await e('plugin:rusqlite|load', { db: s });
                return new t(n);
            }
            static get(e) {
                return new t(e);
            }
            async execute(t, s) {
                const [n, r] = await e('plugin:rusqlite|execute', {
                    db: this.path,
                    query: t,
                    values: s ?? [],
                });
                return { lastInsertId: r, rowsAffected: n };
            }
            async select(t, s) {
                return await e('plugin:rusqlite|select', {
                    db: this.path,
                    query: t,
                    values: s ?? [],
                });
            }
            async close(t) {
                return await e('plugin:rusqlite|close', { db: t });
            }

            async beginTransaction(t) {
                const [n] = await e('plugin:rusqlite|begin_transaction', { db: this.path });
                return { txId: n }
            }

            async commitTransaction(t) {
                await e('plugin:rusqlite|commit_transaction', { txId: t });
            }

            async rollbackTransaction(t) {
                await e('plugin:rusqlite|rollback_transaction', { txId: t });
            }
            async migrateToVersion(t) {
                await e('plugin:rusqlite|migrate', { version: t, db: this.path });
            }
        }
        return t;
    })();
    Object.defineProperty(window.__TAURI__, 'rusqlite', {
        value: __TAURI_PLUGIN_RUSQLITE__,
    });
}
