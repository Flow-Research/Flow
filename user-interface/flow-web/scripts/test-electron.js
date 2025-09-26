#!/usr/bin/env node

// Simple test script to verify Electron can start
const { spawn } = require("child_process");
const path = require("path");

console.log("Testing Electron setup...");
console.log("Building application first...");

const buildProcess = spawn("npm", ["run", "build"], {
  stdio: "inherit",
  cwd: process.cwd(),
});

buildProcess.on("close", (code) => {
  if (code !== 0) {
    console.error("Build failed!");
    process.exit(1);
  }

  console.log("Build successful! Testing Electron...");

  const electronProcess = spawn("npx", ["electron", "public/electron.cjs"], {
    stdio: "inherit",
    cwd: process.cwd(),
  });

  // Auto-close after 5 seconds for testing
  setTimeout(() => {
    console.log("Test completed - closing Electron...");
    electronProcess.kill();
    process.exit(0);
  }, 5000);

  electronProcess.on("close", () => {
    console.log("Electron closed.");
    process.exit(0);
  });
});
