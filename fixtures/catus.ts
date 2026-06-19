import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

// Repo root is one level up from fixtures/.
const REPO_ROOT = resolve(__dirname, "..");
const CATUS_BIN = resolve(REPO_ROOT, "target", "debug", "catus");

export interface CatusHandle {
  process: ChildProcess;
  /** Stop the running Catus process. */
  kill: () => void;
}

/**
 * Launch the locally built `catus` binary.
 *
 * The binary is produced by `cargo build` (run automatically via the `pretest`
 * npm script). If it is missing, the error explains how to build it.
 */
export function launchCatus(): CatusHandle {
  if (!existsSync(CATUS_BIN)) {
    throw new Error(
      `Catus binary not found at ${CATUS_BIN}. Run \`cargo build\` first.`,
    );
  }

  const child = spawn(CATUS_BIN, [], {
    cwd: REPO_ROOT,
    stdio: "ignore",
    detached: false,
  });

  const kill = () => {
    if (child.exitCode === null && !child.killed) {
      // Send SIGTERM first for a graceful shutdown, then escalate if needed.
      child.kill("SIGTERM");
      const force = setTimeout(() => {
        if (child.exitCode === null && !child.killed) {
          child.kill("SIGKILL");
        }
      }, 3000);
      child.once("exit", () => clearTimeout(force));
    }
  };

  child.once("error", (err) => {
    console.error("Failed to launch catus:", err);
  });

  return { process: child, kill };
}
