#!/usr/bin/env node

if (process.stdin.isTTY) {
  process.stdin.setRawMode(true);
}

process.stdin.resume();
process.stdout.write("\x1b]0;Echo\x07");

process.stdin.on("data", (chunk) => {
  if (chunk.includes(0x03) || chunk.includes(0x04)) {
    process.exit(0);
  }

  process.stdout.write(chunk);
});
