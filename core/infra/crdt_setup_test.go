package infra

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestSetupCRDT(t *testing.T) {
	// Create a temporary directory for the Badger datastore
	tempDir, err := os.MkdirTemp("", "flow-crdt-test-*")
	require.NoError(t, err, "Failed to create temp dir")
	t.Logf("Using temp directory for Badger: %s", tempDir)

	// Ensure cleanup happens even if test fails
	defer func() {
		err := os.RemoveAll(tempDir)
		if err != nil {
			t.Logf("Warning: failed to remove temp dir %s: %v", tempDir, err)
		}
	}()

	// Define test options
	badgerOpts := BadgerOptions{
		Path: tempDir, // Correct field name
	}
	// Explicitly request ephemeral port listening
	lp2pOpts := Libp2pOptions{
		ListenAddrs: []string{"/ip4/0.0.0.0/tcp/0"}, // Correct field name and value
		// BootstrapPeers and PubSubTopic are handled internally by SetupCRDT
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second) // Add timeout
	defer cancel()

	// Setup CRDT Services
	t.Log("Setting up CRDT services...")
	services, err := SetupCRDT(ctx, badgerOpts, lp2pOpts) // Corrected arguments
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
