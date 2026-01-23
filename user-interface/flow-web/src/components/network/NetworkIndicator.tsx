import { useNetwork } from '../../contexts/NetworkContext';
import './NetworkIndicator.css';

interface NetworkIndicatorProps {
  /** Whether to show peer count */
  showPeerCount?: boolean;
  /** Click handler to open details */
  onClick?: () => void;
}

/**
 * Compact network status indicator for the sidebar/header.
 * Shows connection status with optional peer count.
 */
export function NetworkIndicator({
  showPeerCount = true,
  onClick,
}: NetworkIndicatorProps) {
  const { isConnected, peerCount, isLoading } = useNetwork();

  // Determine status type
  let statusType: 'connected' | 'disconnected' | 'unknown' = 'unknown';
  if (isLoading) {
    statusType = 'unknown';
  } else if (isConnected) {
    statusType = 'connected';
  } else {
    statusType = 'disconnected';
  }

  const statusLabel =
    statusType === 'connected'
      ? showPeerCount
        ? `${peerCount} peer${peerCount !== 1 ? 's' : ''}`
        : 'Connected'
      : statusType === 'disconnected'
      ? 'Offline'
      : '...';

  const isClickable = !!onClick;

  const Wrapper = isClickable ? 'button' : 'div';

  return (
    <Wrapper
      className={`network-indicator network-indicator--${statusType} ${
        isClickable ? 'network-indicator--clickable' : ''
      }`}
      onClick={onClick}
      {...(isClickable && {
        type: 'button' as const,
        'aria-label': `Network status: ${statusLabel}. Click for details.`,
      })}
    >
      <span
        className={`network-indicator__dot network-indicator__dot--${statusType}`}
        aria-hidden="true"
      />
      <span className="network-indicator__label">{statusLabel}</span>
    </Wrapper>
  );
}
