import { useAuth } from '../contexts/AuthContext';
import './Settings.css';

export function SettingsPage() {
  const { user } = useAuth();

  return (
    <div className="settings-page">
      <h1>Settings</h1>

      <section className="settings-section">
        <h2>Account</h2>
        <div className="settings-card">
          <div className="setting-row">
            <span className="setting-label">DID</span>
            <code className="setting-value">{user?.did}</code>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2>About</h2>
        <div className="settings-card">
          <div className="setting-row">
            <span className="setting-label">Version</span>
            <span className="setting-value">0.1.0 (MVP)</span>
          </div>
          <div className="setting-row">
            <span className="setting-label">Backend</span>
            <span className="setting-value">localhost:8080</span>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2>Data</h2>
        <div className="settings-card">
          <p className="settings-info">
            Your data is stored locally on your device. No data is sent to external
            servers except when querying the local LLM for answers.
          </p>
        </div>
      </section>
    </div>
  );
}
