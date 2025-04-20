package storage

import (
	"context"
	"os"
	"testing"

	"flow-network/flow/core/types"
	"flow-network/flow/core/infra"

	"github.com/ipfs/go-cid"
	"github.com/ipfs/go-datastore"
	"github.com/stretchr/testify/require"
	mh "github.com/multiformats/go-multihash"
)

// createTestFlowObject creates a sample FlowObject with a deterministic CID.
func createTestFlowObject(t *testing.T, state []byte, meta map[string]interface{}) *types.FlowObject {
	// Create a CIDv1 with raw codec and sha2-256 hash of the state
	// This provides a somewhat realistic, deterministic ID based on content.
	// If state is nil or empty, use a predefined prefix for a placeholder CID.
	var objCid cid.Cid
	var err error
	if len(state) > 0 {
		h, _ := mh.Sum(state, mh.SHA2_256, -1)
		objCid = cid.NewCidV1(cid.Raw, h)
	} else {
		// Placeholder CID for empty state
		objCid, err = cid.Decode("bafkreibm6f2qmjgyqnknj2fwaeizm6bwgyrhmrttrut5qgkk7isaahtdki")
		require.NoError(t, err)
	}

	obj := &types.FlowObject{
		ID:         objCid,
		ObjectType: "test-object",
		Metadata:   meta,
		State:      state,
		Sequence:   1,
	}
	t.Logf("Created test object with ID: %s", obj.ID)
	return obj
}

func TestSaveLoadFlowObject(t *testing.T) {
	ctx := context.Background()

	// --- Setup using infra.SetupCRDT ---
	tempDir, err := os.MkdirTemp(os.TempDir(), "flow-op-test-*")
	require.NoError(t, err, "Failed to create temp dir")
	t.Logf("Using temp directory for Badger: %s", tempDir)

	badgerOpts := infra.BadgerOptions{Path: tempDir}
	// Use placeholder Libp2p options; networking isn't tested here
	p2pOpts := infra.Libp2pOptions{}

	services, err := infra.SetupCRDT(ctx, badgerOpts, p2pOpts)
	require.NoError(t, err, "infra.SetupCRDT failed")
	require.NotNil(t, services, "infra.SetupCRDT returned nil services")

	// Ensure cleanup via t.Cleanup, which calls services.Close()
	t.Cleanup(func() {
		t.Logf("Closing CRDT services and removing temp directory: %s", tempDir)
		if cerr := services.Close(); cerr != nil {
			t.Errorf("Error closing CRDT services: %v", cerr)
		}
		// Attempt to remove temp dir *after* services (especially badger) are closed
		if rerr := os.RemoveAll(tempDir); rerr != nil {
			t.Errorf("Error removing temp directory %s: %v", tempDir, rerr)
		}
	})

	crdtStore := services.CrdtStore
	baseStore := services.BaseStore
	// --- End Setup ---

	// 1. Test saving and loading a valid object
	t.Run("SaveAndLoadValid", func(t *testing.T) {
		state := []byte("hello world")
		meta := map[string]interface{}{"key": "value", "number": 123.45}
		originalObj := createTestFlowObject(t, state, meta)

		// Save the object
		err := SaveFlowObject(ctx, crdtStore, baseStore, originalObj)
		require.NoError(t, err, "SaveFlowObject failed")

		// Load the object back
		loadedObj, err := LoadFlowObject(ctx, crdtStore, baseStore, originalObj.ID)
		require.NoError(t, err, "LoadFlowObject failed")
		require.NotNil(t, loadedObj, "Loaded object is nil")

		// Verify the loaded object matches the original
		require.Equal(t, originalObj.ID, loadedObj.ID, "ID mismatch")
		require.Equal(t, originalObj.ObjectType, loadedObj.ObjectType, "ObjectType mismatch")
		require.Equal(t, originalObj.Metadata, loadedObj.Metadata, "Metadata mismatch")
		require.Equal(t, originalObj.Sequence, loadedObj.Sequence, "Sequence mismatch")
		require.Equal(t, originalObj.State, loadedObj.State, "State mismatch")
	})

	// 2. Test loading a non-existent object
	t.Run("LoadNonExistent", func(t *testing.T) {
		// Create a random CID that shouldn't exist
		h, _ := mh.Sum([]byte("nonexistent"), mh.SHA2_256, -1)
		nonExistentID := cid.NewCidV1(cid.Raw, h)

		loadedObj, err := LoadFlowObject(ctx, crdtStore, baseStore, nonExistentID)
		require.ErrorIs(t, err, datastore.ErrNotFound, "Expected ErrNotFound for non-existent object")
		require.Nil(t, loadedObj, "Loaded object should be nil for non-existent ID")
	})

	// 3. Test saving with undefined ID
	t.Run("SaveUndefinedID", func(t *testing.T) {
		obj := &types.FlowObject{ID: types.UndefObjectId}
		err := SaveFlowObject(ctx, crdtStore, baseStore, obj)
		require.Error(t, err, "Expected error when saving with undefined ID")
	})

	// 4. Test loading with undefined ID
	t.Run("LoadUndefinedID", func(t *testing.T) {
		_, err := LoadFlowObject(ctx, crdtStore, baseStore, types.UndefObjectId)
		require.Error(t, err, "Expected error when loading with undefined ID")
	})

	// 5. Test saving a nil object
	t.Run("SaveNilObject", func(t *testing.T) {
		err := SaveFlowObject(ctx, crdtStore, baseStore, nil)
		require.Error(t, err, "Expected error when saving nil object")
	})

	// 6. Test object with nil state
	t.Run("SaveAndLoadNilState", func(t *testing.T) {
		originalObj := createTestFlowObject(t, nil, map[string]interface{}{"status": "initialized"})

		err := SaveFlowObject(ctx, crdtStore, baseStore, originalObj)
		require.NoError(t, err, "SaveFlowObject with nil state failed")

		loadedObj, err := LoadFlowObject(ctx, crdtStore, baseStore, originalObj.ID)
		require.NoError(t, err, "LoadFlowObject with nil state failed")
		require.NotNil(t, loadedObj)
		require.Equal(t, originalObj.ID, loadedObj.ID)
		// Note: The loaded state might be an empty slice `[]byte{}` instead of `nil`
		// depending on how `crdtStore.Get` handles not found vs empty value.
		// The current LoadFlowObject handles stateNotFound by setting state to nil.
		require.Nil(t, loadedObj.State, "Expected loaded state to be nil when originally saved as nil and not found in CRDT store")
		require.Equal(t, originalObj.Metadata, loadedObj.Metadata)
	})

	// 7. Test object with empty state
	t.Run("SaveAndLoadEmptyState", func(t *testing.T) {
		originalObj := createTestFlowObject(t, []byte{}, map[string]interface{}{"status": "empty"})
		t.Logf("Testing empty state with ID: %s", originalObj.ID)

		err := SaveFlowObject(ctx, crdtStore, baseStore, originalObj)
		require.NoError(t, err, "SaveFlowObject with empty state failed")

		loadedObj, err := LoadFlowObject(ctx, crdtStore, baseStore, originalObj.ID)
		require.NoError(t, err, "LoadFlowObject with empty state failed")
		require.NotNil(t, loadedObj, "Expected loaded object not to be nil")
		require.Equal(t, originalObj.ID, loadedObj.ID)
		require.Len(t, loadedObj.State, 0, "Expected loaded state to be empty slice")
		require.Equal(t, originalObj.Metadata, loadedObj.Metadata)
	})
}
