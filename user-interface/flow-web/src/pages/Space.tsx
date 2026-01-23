import { useState, useEffect, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { api, Space as SpaceType, Entity } from '../services/api';
import { PublishButton, SyncStatus } from '../components/space';
import { useSpaceStatus } from '../hooks/useSpaceStatus';
import './Space.css';

type Tab = 'chat' | 'entities';

export function SpacePage() {
  const { spaceKey } = useParams<{ spaceKey: string }>();
  const navigate = useNavigate();
  const [space, setSpace] = useState<SpaceType | null>(null);
  const [entities, setEntities] = useState<Entity[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<Tab>('chat');
  const [query, setQuery] = useState('');
  const [answer, setAnswer] = useState<string | null>(null);
  const [isQuerying, setIsQuerying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Use the custom hook for space status with polling
  const {
    status: syncStatus,
    isLoading: isSyncLoading,
    error: syncError,
    refresh: refreshStatus,
    reindex,
    isReindexing,
  } = useSpaceStatus(spaceKey || '');

  useEffect(() => {
    if (spaceKey) {
      loadSpace();
    }
  }, [spaceKey]);

  const loadSpace = async () => {
    if (!spaceKey) return;

    try {
      const spaceData = await api.spaces.get(spaceKey).catch(() => null);
      if (spaceData) {
        setSpace(spaceData);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load space');
    } finally {
      setIsLoading(false);
    }
  };

  const loadEntities = async () => {
    if (!spaceKey) return;

    try {
      const data = await api.entities.list(spaceKey);
      setEntities(data);
    } catch {
      setEntities([]);
    }
  };

  useEffect(() => {
    if (activeTab === 'entities' && spaceKey) {
      loadEntities();
    }
  }, [activeTab, spaceKey]);

  const handleQuery = async () => {
    if (!query.trim() || !spaceKey) return;

    setIsQuerying(true);
    setAnswer(null);
    setError(null);

    try {
      const result = await api.query.search(spaceKey, query);
      setAnswer(result.response);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Query failed');
    } finally {
      setIsQuerying(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleQuery();
    }
  };

  const handleDelete = async () => {
    if (!spaceKey) return;
    if (!window.confirm(`Are you sure you want to delete space "${spaceKey}"?`)) return;

    try {
      await api.spaces.delete(spaceKey);
      navigate('/');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete space');
    }
  };

  const handlePublishStatusChange = useCallback(
    (isPublished: boolean, publishedAt?: string) => {
      setSpace((prev) =>
        prev ? { ...prev, is_published: isPublished, published_at: publishedAt ?? null } : prev
      );
    },
    []
  );

  if (isLoading) {
    return (
      <div className="space-page">
        <div className="loading">Loading space...</div>
      </div>
    );
  }

  return (
    <div className="space-page">
      <div className="space-header">
        <div className="space-title">
          <button onClick={() => navigate('/')} className="back-btn">
            ←
          </button>
          <h1>{spaceKey}</h1>
          {syncStatus?.indexing_in_progress && (
            <span className="indexing-badge">Indexing...</span>
          )}
        </div>
        <div className="space-actions">
          {space && (
            <PublishButton
              spaceKey={spaceKey!}
              spaceName={space.name || spaceKey!}
              isPublished={space.is_published ?? false}
              publishedAt={space.published_at}
              onStatusChange={handlePublishStatusChange}
            />
          )}
          <button onClick={handleDelete} className="delete-btn">
            Delete
          </button>
        </div>
      </div>

      <SyncStatus
        status={syncStatus}
        isLoading={isSyncLoading}
        error={syncError}
        isReindexing={isReindexing}
        onReindex={reindex}
        onRefresh={refreshStatus}
        showReindex
      />

      <div className="space-tabs">
        <button
          className={`tab ${activeTab === 'chat' ? 'active' : ''}`}
          onClick={() => setActiveTab('chat')}
        >
          💬 Chat
        </button>
        <button
          className={`tab ${activeTab === 'entities' ? 'active' : ''}`}
          onClick={() => setActiveTab('entities')}
        >
          🔗 Entities
        </button>
      </div>

      {error && <div className="error-message">{error}</div>}

      {activeTab === 'chat' && (
        <div className="chat-container">
          <div className="chat-messages">
            {answer && (
              <div className="message assistant">
                <div className="message-content">{answer}</div>
              </div>
            )}
            {!answer && !isQuerying && (
              <div className="chat-empty">
                Ask a question about your documents...
              </div>
            )}
            {isQuerying && (
              <div className="message assistant loading">
                <div className="message-content">Thinking...</div>
              </div>
            )}
          </div>
          <div className="chat-input-container">
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Ask a question..."
              className="chat-input"
              disabled={isQuerying}
            />
            <button
              onClick={handleQuery}
              disabled={!query.trim() || isQuerying}
              className="send-btn"
            >
              {isQuerying ? '...' : '→'}
            </button>
          </div>
        </div>
      )}

      {activeTab === 'entities' && (
        <div className="entities-container">
          {entities.length === 0 ? (
            <div className="entities-empty">
              No entities extracted yet. Index some documents first.
            </div>
          ) : (
            <div className="entities-list">
              {entities.map((entity) => (
                <div key={entity.id} className="entity-card">
                  <div className="entity-type">{entity.entity_type}</div>
                  <div className="entity-name">{entity.name}</div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
