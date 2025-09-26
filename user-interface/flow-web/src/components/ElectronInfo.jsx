import { useState, useEffect } from "react";
import "./ElectronInfo.css";

function ElectronInfo() {
  const [appInfo, setAppInfo] = useState({
    version: "N/A",
    platform: "N/A",
    isElectron: false,
  });

  useEffect(() => {
    const loadAppInfo = async () => {
      // Check if we're running in Electron
      if (window.electronAPI) {
        try {
          const version = await window.electronAPI.getAppVersion();
          const platform = await window.electronAPI.getPlatform();

          setAppInfo({
            version,
            platform,
            isElectron: true,
          });
        } catch (error) {
          console.error("Error getting app info:", error);
        }
      } else if (window.environment?.isElectron) {
        setAppInfo({
          version: "Unknown",
          platform: window.environment.platform || "Unknown",
          isElectron: true,
        });
      }
    };

    loadAppInfo();
  }, []);

  if (!appInfo.isElectron) {
    return (
      <div className="electron-info web-mode">
        <h3>🌐 Web Mode</h3>
        <p>Running in browser - Electron features not available</p>
      </div>
    );
  }

  return (
    <div className="electron-info desktop-mode">
      <h3>🖥️ Desktop Mode</h3>
      <div className="info-grid">
        <div className="info-item">
          <span className="label">Version:</span>
          <span className="value">{appInfo.version}</span>
        </div>
        <div className="info-item">
          <span className="label">Platform:</span>
          <span className="value">{appInfo.platform}</span>
        </div>
        {window.environment && (
          <>
            <div className="info-item">
              <span className="label">Electron:</span>
              <span className="value">
                {window.environment.electronVersion}
              </span>
            </div>
            <div className="info-item">
              <span className="label">Node.js:</span>
              <span className="value">{window.environment.nodeVersion}</span>
            </div>
            <div className="info-item">
              <span className="label">Chrome:</span>
              <span className="value">{window.environment.chromeVersion}</span>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export default ElectronInfo;
