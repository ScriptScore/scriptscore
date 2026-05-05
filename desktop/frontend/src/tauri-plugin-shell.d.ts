// SPDX-License-Identifier: AGPL-3.0-only
declare module '@tauri-apps/plugin-shell' {
  export function open(path: string): Promise<void>;
}

