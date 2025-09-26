#!/usr/bin/env node

const { spawn } = require("child_process");
const { createServer } = require("vite");
const electron = require("electron");
const path = require("path");

let electronProcess = null;
let manualRestart = false;

function startElectron() {
  if (electronProcess) {
    manualRestart = true;
    process.kill(electronProcess.pid);
    electronProcess = null;
    startElectron();
    return;
  }

  electronProcess = spawn(
    electron,
    [path.join(__dirname, "../public/electron.cjs")],
    {
      stdio: "inherit",
    }
  );

  electronProcess.on("close", () => {
    if (!manualRestart) {
      process.exit();
    }
    manualRestart = false;
  });
}

async function startVite() {
  const server = await createServer({
    // any valid user config options, plus `mode` and `configFile`
    configFile: path.join(__dirname, "../vite.config.js"),
    server: {
      port: 3000,
    },
  });

  await server.listen();

  console.log("Vite dev server running on http://localhost:3000");

  // Start Electron after Vite is ready
  setTimeout(startElectron, 2000);
}

// Handle process termination
process.on("SIGINT", () => {
  if (electronProcess) {
    electronProcess.kill();
  }
  process.exit();
});

process.on("SIGTERM", () => {
  if (electronProcess) {
    electronProcess.kill();
  }
  process.exit();
});

startVite().catch(console.error);
