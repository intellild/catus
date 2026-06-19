import { appendFileSync, existsSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";

// Log capture is opt-in: set CATUS_LOG_DIR to a directory path and the e2e
// harness will append timestamped orchestration logs to <dir>/e2e.log. The
// catus binary writes its own logs to <dir>/catus.log when it sees the same
// variable. Leave unset to disable (the app then logs to stdout/stderr).
//
// Example:
//   CATUS_LOG_DIR=logs pnpm test
const LOG_DIR = process.env.CATUS_LOG_DIR?.trim();

export const debugEnabled = Boolean(LOG_DIR);

const LOG_FILE = LOG_DIR ? resolve(LOG_DIR, "e2e.log") : null;

function ensureDir(): void {
  if (LOG_DIR && !existsSync(LOG_DIR)) {
    mkdirSync(LOG_DIR, { recursive: true });
  }
}

/** Append a timestamped line to the e2e log. No-op when logging is disabled. */
export function debug(message: string): void {
  if (!LOG_FILE) return;
  ensureDir();
  const ts = new Date().toISOString();
  appendFileSync(LOG_FILE, `${ts} ${message}\n`, "utf8");
}

/** Absolute path of the e2e log file, or null when logging is disabled. */
export function debugLogPath(): string | null {
  return LOG_FILE;
}
