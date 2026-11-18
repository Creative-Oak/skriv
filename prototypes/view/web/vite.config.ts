// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { defineConfig } from 'vite'

export default defineConfig({
  build: {
    target: 'es2023',
    rollupOptions: { input: { index: 'index.html', a: 'a.html', b: 'b.html' } },
  },
  server: { host: true, port: 5180 },
})
