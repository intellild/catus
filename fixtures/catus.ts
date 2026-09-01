import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { debug, debugEnabled, debugLogPath } from "./log";

const __dirname = dirname(fileURLToPath(import.meta.url));

// Repo root is one level up from fixtures/.
const REPO_ROOT = resolve(__dirname, "..");
const CATUS_BIN = resolve(REPO_ROOT, "target", "debug", "catus");
export const ECHO_PTY_SCRIPT = resolve(REPO_ROOT, "scripts", "echo-pty.js");

export interface CatusHandle {
  process: ChildProcess;
  /** Stop the running Catus process. */
  kill: () => Promise<void>;
}

export interface LaunchCatusOptions {
  localPtyProgram?: string;
  localPtyArgs?: string[];
}

/**
 * Launch the locally built `catus` binary.
 *
 * The binary is produced by `cargo build` (run automatically via the `pretest`
 * npm script). If it is missing, the error explains how to build it.
 *
 * When `CATUS_LOG_DIR` is set, the child process stdout/stderr are
 * captured into the e2e log (in addition to orchestration events).
 */
export function launchCatus(options: LaunchCatusOptions = {}): CatusHandle {
  if (!existsSync(CATUS_BIN)) {
    throw new Error(
      `Catus binary not found at ${CATUS_BIN}. Run \`cargo build\` first.`,
    );
  }

  debug(`launching catus binary: ${CATUS_BIN} (cwd: ${REPO_ROOT})`);
  if (debugEnabled) {
    debug(`e2e log -> ${debugLogPath()}`);
  }

  // Pipe stdio only when debugging so we can capture the app's output.
  const stdio: "ignore" | ["ignore", "pipe", "pipe"] = debugEnabled
    ? ["ignore", "pipe", "pipe"]
    : "ignore";

  const env = { ...process.env };
  if (options.localPtyProgram) {
    env.CATUS_LOCAL_PTY_PROGRAM = options.localPtyProgram;
    env.CATUS_LOCAL_PTY_ARGS = JSON.stringify(options.localPtyArgs ?? []);
  }

  const child = spawn(CATUS_BIN, [], {
    cwd: REPO_ROOT,
    env,
    stdio,
    detached: false,
  });

  if (debugEnabled) {
    const write = (stream: "stdout" | "stderr") => (data: Buffer) => {
      for (const line of data.toString("utf8").split(/\r?\n/)) {
        if (line.length) debug(`[catus:${stream}] ${line}`);
      }
    };
    child.stdout?.on("data", write("stdout"));
    child.stderr?.on("data", write("stderr"));
  }

  let stopping: Promise<void> | undefined;
  const kill = (): Promise<void> => {
    if (stopping) return stopping;

    stopping = new Promise((resolve) => {
      if (child.exitCode !== null) {
        resolve();
        return;
      }

      let giveUp: ReturnType<typeof setTimeout> | undefined;
      const force = setTimeout(() => {
        if (child.exitCode === null) {
          debug("catus did not exit, escalating to SIGKILL");
          child.kill("SIGKILL");
          giveUp = setTimeout(() => {
            debug("catus did not report exit after SIGKILL");
            resolve();
          }, 1000);
        }
      }, 3000);

      child.once("exit", (code, signal) => {
        clearTimeout(force);
        if (giveUp) clearTimeout(giveUp);
        debug(`catus exited (code=${code}, signal=${signal})`);
        resolve();
      });

      debug("stopping catus (SIGTERM)");
      child.kill("SIGTERM");
    });

    return stopping;
  };

  child.once("error", (err) => {
    debug(`failed to launch catus: ${err}`);
    console.error("Failed to launch catus:", err);
  });

  child.once("exit", (code, signal) => {
    debug(`catus process exited early (code=${code}, signal=${signal})`);
  });

  return { process: child, kill };
}
