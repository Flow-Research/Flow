package infra

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestSetupCRDT(t *testing.T) {
	ctx := context.Background()

	tempDir, err := os.MkdirTemp(os.TempDir(), "crdt-setup-test-*")
	require.NoError(t, err, "Failed to create temp dir")
	t.Logf("Using temp directory for Badger: %s", tempDir)

	// Cleanup function to remove the temp directory
	t.Cleanup(func() {
		t.Logf("Cleaning up temp directory: %s", tempDir)
		if rerr := os.RemoveAll(tempDir); rerr != nil {
			t.Errorf("Error removing temp directory %s: %v", tempDir, rerr)
		}
	})

	// Configure options for SetupCRDT
	badgerOpts := BadgerOptions{
		Path: tempDir,
	}
	// Use placeholder Libp2p options for now, as we aren't testing networking itself here
	p2pOpts := Libp2pOptions{
		ListenAddrs: []string{"/ip4/0.0.0.0/tcp/0"},
		// BootstrapPeers and PubSubTopic are handled internally by SetupCRDT
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second) // Add timeout
	defer cancel()

	// Setup CRDT Services
	t.Log("Setting up CRDT services...")
	services, err := SetupCRDT(ctx, badgerOpts, p2pOpts) // Corrected arguments
	require.NoError(t, err, "SetupCRDT failed")
	require.NotNil(t, services, "SetupCRDT returned nil services")

	// Verify components are initialized
	require.NotNil(t, services.CrdtStore, "CRDT Store should not be nil")
	require.NotNil(t, services.BaseStore, "Base Store should not be nil")
	require.NotNil(t, services.Host, "Libp2p Host should not be nil")
	require.NotNil(t, services.PubSub, "PubSub should not be nil")
	require.NotNil(t, services.DAGService, "DAG Service should not be nil")
	require.NotNil(t, services.BlockService, "Block Service should not be nil")

	// Close the services
	err = services.Close()
	require.NoError(t, err, "Failed to close services")

	t.Log("CRDT services closed successfully.")
}
