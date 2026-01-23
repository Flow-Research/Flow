import { createContext, useContext, useMemo } from 'react';
import { useNetworkStatus, UseNetworkStatusReturn } from '../hooks/useNetworkStatus';

/** Network context type */
type NetworkContextType = UseNetworkStatusReturn;

const NetworkContext = createContext<NetworkContextType | undefined>(undefined);

interface NetworkProviderProps {
  children: React.ReactNode;
}

/**
 * Provider component that makes network status available to the entire app.
 * Should be placed high in the component tree (e.g., in main.tsx or App.tsx).
 */
export function NetworkProvider({ children }: NetworkProviderProps) {
  const networkStatus = useNetworkStatus();

  // Memoize the context value to prevent unnecessary re-renders
  const value = useMemo(
    () => networkStatus,
    [
      networkStatus.isConnected,
      networkStatus.peerCount,
      networkStatus.peers,
      networkStatus.isLoading,
      networkStatus.error,
      networkStatus.refresh,
    ]
  );

  return (
    <NetworkContext.Provider value={value}>
      {children}
    </NetworkContext.Provider>
  );
}

/**
 * Hook to access network status from any component.
 * Must be used within a NetworkProvider.
 */
export function useNetwork(): NetworkContextType {
  const context = useContext(NetworkContext);
  if (context === undefined) {
    throw new Error('useNetwork must be used within a NetworkProvider');
  }
  return context;
}
