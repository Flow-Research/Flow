import { useState, useEffect } from 'react';
import { Link } from 'react-router-dom';
import { api, Space } from '../services/api';
import './Home.css';

export function HomePage() {
  const [spaces, setSpaces] = useState<Space[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [newSpaceDir, setNewSpaceDir] = useState('');
  const [isCreating, setIsCreating] = useState(false);

  useEffect(() => {
    loadSpaces();
  }, []);

  const loadSpaces = async () => {
    try {
      const data = await api.spaces.list();
      setSpaces(data);
    } catch (err) {
      if (err instanceof Error && err.message.includes('404')) {
        setSpaces([]);
      } else {
        setError(err instanceof Error ? err.message : 'Failed to load spaces');
      }
    } finally {
      setIsLoading(false);
    }
  };

  const handleCreateSpace = async () => {
    if (!newSpaceDir.trim()) return;

    setIsCreating(true);
    setError(null);

    try {
      await api.spaces.create(newSpaceDir);
      setShowCreateModal(false);
      setNewSpaceDir('');
      loadSpaces();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create space');
    } finally {
      setIsCreating(false);
    }
  };

  if (isLoading) {
    return (
      <div className="home-page">
        <div className="loading">Loading spaces...</div>
      </div>
    );
  }

  return (
    <div className="home-page">
      <div className="home-header">
        <h1>Your Spaces</h1>
        <button onClick={() => setShowCreateModal(true)} className="create-btn">
          + Create Space
        </button>
      </div>

      {error && <div className="error-message">{error}</div>}

      {spaces.length === 0 ? (
        <div className="empty-state">
          <div className="empty-icon">📁</div>
          <h2>No spaces yet</h2>
          <p>Create your first space to start indexing documents.</p>
          <button onClick={() => setShowCreateModal(true)} className="create-btn primary">
            Create Your First Space
          </button>
        </div>
      ) : (
        <div className="spaces-grid">
          {spaces.map((space) => (
            <Link to={`/spaces/${space.key}`} key={space.key} className="space-card">
              <div className="space-icon">📂</div>
              <div className="space-info">
                <h3>{space.key}</h3>
                <p className="space-location">{space.location}</p>
                <p className="space-date">
                  Created {new Date(space.time_created).toLocaleDateString()}
                </p>
              </div>
            </Link>
          ))}
        </div>
      )}

      {showCreateModal && (
        <div className="modal-overlay" onClick={() => setShowCreateModal(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>Create New Space</h2>
            <p>Enter the local directory path to index:</p>
            <input
              type="text"
              value={newSpaceDir}
              onChange={(e) => setNewSpaceDir(e.target.value)}
              placeholder="/path/to/your/documents"
              className="modal-input"
              autoFocus
            />
            <div className="modal-actions">
              <button onClick={() => setShowCreateModal(false)} className="modal-btn secondary">
                Cancel
              </button>
              <button
                onClick={handleCreateSpace}
                disabled={!newSpaceDir.trim() || isCreating}
                className="modal-btn primary"
              >
                {isCreating ? 'Creating...' : 'Create Space'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
