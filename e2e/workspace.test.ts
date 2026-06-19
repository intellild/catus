import { afterAll, beforeAll, describe, expect, it } from "@rstest/core";
import { type ComputerAgent, agentForComputer } from "@midscene/computer";
import { launchCatus, type CatusHandle } from "../fixtures/catus";
import { debug, debugEnabled } from "../fixtures/log";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Run an AI action with debug logging around it. */
async function act(agent: ComputerAgent, prompt: string): Promise<void> {
  debug(`aiAct: ${prompt}`);
  await agent.aiAct(prompt);
  debug(`aiAct done: ${prompt}`);
}

async function assert_(agent: ComputerAgent, assertion: string): Promise<void> {
  debug(`aiAssert: ${assertion}`);
  await agent.aiAssert(assertion);
  debug(`aiAssert ok: ${assertion}`);
}

describe("catus workspace UI", () => {
  let agent: ComputerAgent;
  let catus: CatusHandle;

  beforeAll(async () => {
    debug("=== catus workspace UI: beforeAll ===");
    catus = launchCatus();
    agent = await agentForComputer({
      aiActionContext:
        'You are controlling Catus, a macOS terminal client. It has a left sidebar listing workspaces, a title bar with tabs and a "+" button to add a terminal tab, and a terminal area on the right.',
    });

    // Wait for the Catus window to appear on screen.
    debug("aiWaitFor: catus window visible");
    await agent.aiWaitFor(
      "A Catus terminal application window is visible with a left sidebar showing workspaces and a terminal area",
      { timeoutMs: 30_000 },
    );
    debug("aiWaitFor ok: catus window visible");
  });

  afterAll(() => {
    debug("=== catus workspace UI: afterAll ===");
    catus?.kill();
  });

  it("shows the default Local workspace in the sidebar", async () => {
    const names = await agent.aiQuery<string[]>(
      "string[], the workspace names listed in the left sidebar",
    );
    debug(`aiQuery workspace names: ${JSON.stringify(names)}`);
    expect(names).toContain("Local");
  });

  it("has an Add Workspace button in the sidebar", async () => {
    await assert_(
      agent,
      'There is an "Add Workspace" button at the bottom of the left sidebar',
    );
  });

  it("opens the Add Workspace dialog when clicking Add Workspace", async () => {
    await act(
      agent,
      'Click the "Add Workspace" button at the bottom of the left sidebar',
    );
    await sleep(500);
    await assert_(
      agent,
      'An "Add Workspace" dialog is open with Type and Command fields',
    );
  });

  it("keeps at least one workspace after closing", async () => {
    // Close the dialog first (Cancel / Escape) so we're back at the main window.
    await act(agent, "Press Escape to close the Add Workspace dialog");
    await sleep(500);

    // The single default workspace must not be closeable.
    await assert_(
      agent,
      'The left sidebar still shows the "Local" workspace and there is no close button on it',
    );
  }, 240_000);

  it("adds a new terminal tab when clicking the + button in the title bar", async () => {
    // Sanity check: start from a single tab.
    const initialCount = await agent.aiQuery<number>(
      "number, how many terminal tabs are shown in the title bar tab strip",
    );
    debug(`initial tab count: ${initialCount}`);
    expect(initialCount).toBe(1);

    // Capture the existing tab's title before adding a new one.
    const initialTitle = await agent.aiQuery<string>(
      "string, the title text of the first (only) terminal tab in the title bar",
    );
    debug(`initial tab title: ${JSON.stringify(initialTitle)}`);

    // The "+" button sits in the title bar, right after the tab strip. It is
    // NOT the "Add Workspace" button in the sidebar.
    await act(
      agent,
      'Click the small "+" button in the title bar, immediately to the right of the terminal tab strip, to add a new terminal tab',
    );
    await sleep(800);

    // Wait until a second tab appears.
    debug("aiWaitFor: two terminal tabs visible");
    await agent.aiWaitFor(
      "There are exactly two terminal tabs in the title bar",
      {
        timeoutMs: 30_000,
      },
    );
    debug("aiWaitFor ok: two terminal tabs visible");

    const tabCount = await agent.aiQuery<number>(
      "number, how many terminal tabs are shown in the title bar tab strip",
    );
    debug(`tab count after add: ${tabCount}`);
    expect(tabCount).toBe(2);

    // The new tab must carry a proper title. Both tabs run the same local
    // shell, so their titles should match the initial tab's title.
    const titles = await agent.aiQuery<string[]>(
      "string[], the title text of every terminal tab in the title bar, in left-to-right order",
    );
    debug(`tab titles: ${JSON.stringify(titles)}`);
    expect(titles).toHaveLength(2);
    for (const title of titles) {
      expect(typeof title).toBe("string");
      expect(title.trim().length).toBeGreaterThan(0);
    }
    // New tab should share the same shell title as the original tab.
    expect(titles[1]).toBe(initialTitle);
    expect(titles[0]).toBe(initialTitle);
  }, 240_000);
});

// Keep an explicit marker in the log so debug runs are easy to grep.
if (debugEnabled) {
  debug("debug mode enabled via CATUS_LOG_DIR");
}
