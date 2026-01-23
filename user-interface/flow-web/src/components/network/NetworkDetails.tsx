import { useNetwork } from '../../contexts/NetworkContext';
import './NetworkDetails.css';

/**
 * Detailed network status panel for the Settings page.
 * Shows connection status, peer list, and allows manual refresh.
 */
export function NetworkDetails() {
  const { isConnected, peerCount, peers, isLoading, error, refresh } = useNetwork();

  const handleRefresh = () => {
    refresh();
  };

  // Format peer connection time
  const formatConnectionTime = (connectedAt: string) => {
    const date = new Date(connectedAt);
    return date.toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  // Truncate peer ID for display
  const truncatePeerId = (peerId: string) => {
    if (peerId.length <= 20) return peerId;
    return `${peerId.slice(0, 8)}...${peerId.slice(-8)}`;
  };

  return (
    <div className="network-details">
      <div className="network-details__header">
        <h3 className="network-details__title">Network Status</h3>
        <button
          type="button"
          className="network-details__refresh-btn"
          onClick={handleRefresh}
          disabled={isLoading}
          aria-label="Refresh network status"
        >
          {isLoading ? 'Refreshing...' : '🔄 Refresh'}
        </button>
      </div>

      <div className="network-details__status">
        <div className="network-details__status-row">
          <span className="network-details__label">Connection</span>
          <span
            className={`network-details__value network-details__value--${
              isConnected ? 'connected' : 'disconnected'
            }`}
          >
            <span
              className={`network-details__dot network-details__dot--${
                isConnected ? 'connected' : 'disconnected'
              }`}
            />
            {isConnected ? 'Connected' : 'Disconnected'}
          </span>
        </div>
        <div className="network-details__status-row">
          <span className="network-details__label">Peers</span>
          <span className="network-details__value">{peerCount}</span>
        </div>
      </div>

      {error && (
        <div className="network-details__error" role="alert">
          {error}
        </div>
      )}

      {peers.length > 0 && (
        <div className="network-details__peers">
          <h4 className="network-details__peers-title">Connected Peers</h4>
          <ul className="network-details__peer-list">
            {peers.map((peer) => (
              <li key={peer.id} className="network-details__peer">
                <span className="network-details__peer-id" title={peer.id}>
                  {truncatePeerId(peer.id)}
                </span>
                <span className="network-details__peer-time">
                  {formatConnectionTime(peer.connected_at)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {!isLoading && peerCount === 0 && !error && (
        <div className="network-details__empty">
          <p>No peers connected. The network may still be bootstrapping.</p>
        </div>
      )}
    </div>
  );
}
