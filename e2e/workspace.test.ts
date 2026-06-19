import { afterAll, beforeAll, describe, expect, it } from "@rstest/core";
import { type ComputerAgent, agentForComputer } from "@midscene/computer";
import { launchCatus, type CatusHandle } from "../fixtures/catus";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

describe("catus workspace UI", () => {
  let agent: ComputerAgent;
  let catus: CatusHandle;

  beforeAll(async () => {
    catus = launchCatus();
    agent = await agentForComputer({
      aiActionContext:
        'You are controlling Catus, a macOS terminal client. It has a left sidebar listing workspaces, a title bar with tabs and a "+" button to add a terminal tab, and a terminal area on the right.',
    });

    // Wait for the Catus window to appear on screen.
    await agent.aiWaitFor(
      "A Catus terminal application window is visible with a left sidebar showing workspaces and a terminal area",
      { timeoutMs: 30_000 },
    );
  });

  afterAll(() => {
    catus?.kill();
  });

  it("shows the default Local workspace in the sidebar", async () => {
    const names = await agent.aiQuery<string[]>(
      "string[], the workspace names listed in the left sidebar",
    );
    expect(names).toContain("Local");
  });

  it("has an Add Workspace button in the sidebar", async () => {
    await agent.aiAssert(
      'There is an "Add Workspace" button at the bottom of the left sidebar',
    );
  });

  it("opens the Add Workspace dialog when clicking Add Workspace", async () => {
    await agent.aiAct(
      'Click the "Add Workspace" button at the bottom of the left sidebar',
    );
    await sleep(500);
    await agent.aiAssert(
      'An "Add Workspace" dialog is open with Type and Command fields',
    );
  });

  it("keeps at least one workspace after closing", async () => {
    // Close the dialog first (Cancel / Escape) so we're back at the main window.
    await agent.aiAct("Press Escape to close the Add Workspace dialog");
    await sleep(500);

    // The single default workspace must not be closeable.
    await agent.aiAssert(
      'The left sidebar still shows the "Local" workspace and there is no close button on it',
    );
  }, 240_000);
});
