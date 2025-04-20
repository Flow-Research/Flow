package infra

import (
	"context"
	"os"
	"testing"
	"time"

	datastore "github.com/ipfs/go-datastore"
	"github.com/stretchr/testify/require"
)

func TestCRDTSetupAndClose(t *testing.T) {
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
	require.NotNil(t, services.Host, "Libp2p Host should not be nil")
	require.NotNil(t, services.PubSub, "PubSub should not be nil")
	require.NotNil(t, services.BaseStore, "Base Datastore should not be nil")
	// require.NotNil(t, services.Blockstore, "Blockstore should not be nil") // Removed assertion for non-existent field
	require.NotNil(t, services.BlockService, "BlockService should not be nil")
	require.NotNil(t, services.DAGService, "DAGService should not be nil")
	require.NotNil(t, services.CrdtStore, "CRDT Datastore should not be nil")

	t.Logf("Libp2p Host ID: %s", services.Host.ID())
	require.Greater(t, len(services.Host.Addrs()), 0, "Libp2p Host should have listen addresses")
	t.Logf("Libp2p Host listening on: %v", services.Host.Addrs())

	// --- Basic CRDT Store Operation Test ---
	t.Log("Performing basic Put/Get on CRDT store...")
	testKey := datastore.NewKey("test/key1")
	testValue := []byte("hello world")

	err = services.CrdtStore.Put(ctx, testKey, testValue)
	require.NoError(t, err, "Failed to Put value into CRDT store")

	// Short delay to allow potential internal CRDT ops/sync (though unlikely needed in single node test)
	time.Sleep(100 * time.Millisecond)

	retrievedValue, err := services.CrdtStore.Get(ctx, testKey)
	require.NoError(t, err, "Failed to Get value from CRDT store")
	require.Equal(t, testValue, retrievedValue, "Retrieved value does not match original value")
	t.Log("Basic Put/Get successful.")

	// --- Test Closing Services ---
	t.Log("Closing CRDT services...")
	err = services.Close()
	require.NoError(t, err, "Closing services failed")
	t.Log("CRDT services closed successfully.")

	// Optional: Verify badger lock file is released (can be OS dependent)
	// For simplicity, we rely on the NoError check from Close() and the deferred RemoveAll.
}
