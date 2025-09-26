const { contextBridge, ipcRenderer } = require("electron");

// Expose protected methods that allow the renderer process to use
// the ipcRenderer without exposing the entire object
contextBridge.exposeInMainWorld("electronAPI", {
  // App information
  getAppVersion: () => ipcRenderer.invoke("get-app-version"),
  getPlatform: () => ipcRenderer.invoke("get-platform"),

  // Window controls
  minimize: () => ipcRenderer.invoke("window-minimize"),
  maximize: () => ipcRenderer.invoke("window-maximize"),
  close: () => ipcRenderer.invoke("window-close"),

  // File system operations (add as needed)
  openFile: () => ipcRenderer.invoke("dialog-open-file"),
  saveFile: (content) => ipcRenderer.invoke("dialog-save-file", content),

  // Custom Flow-specific APIs can be added here
  // flowAPI: {
  //   createSpace: (data) => ipcRenderer.invoke('flow-create-space', data),
  //   getSpaces: () => ipcRenderer.invoke('flow-get-spaces'),
  // },

  // Event listeners
  onWindowEvent: (callback) => {
    ipcRenderer.on("window-event", callback);
    // Return a function to remove the listener
    return () => ipcRenderer.removeListener("window-event", callback);
  },

  // Utility functions
  openExternal: (url) => ipcRenderer.invoke("open-external", url),
});

// Optional: Add some basic information about the environment
contextBridge.exposeInMainWorld("environment", {
  isElectron: true,
  nodeVersion: process.versions.node,
  chromeVersion: process.versions.chrome,
  electronVersion: process.versions.electron,
});
