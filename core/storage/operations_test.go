package storage

import (
	"context"
	"testing"

	"flow-network/flow/core/types"

	crdt "github.com/ipfs/go-ds-crdt"
	datastore "github.com/ipfs/go-datastore"
	dsync "github.com/ipfs/go-datastore/sync"
	cid "github.com/ipfs/go-cid"
	ipld "github.com/ipfs/go-ipld-format"
	"github.com/stretchr/testify/require"
	mh "github.com/multiformats/go-multihash"
)

// mockDagService provides a minimal implementation of ipld.DAGService for testing.
type mockDagService struct {
	// Add fields if needed for more complex mocks, e.g., a map to store nodes
}

// Compile-time check to ensure mockDagService satisfies ipld.DAGService
var _ ipld.DAGService = (*mockDagService)(nil)

func (m *mockDagService) Add(ctx context.Context, node ipld.Node) error {
	// No-op for this simple mock
	return nil
}

func (m *mockDagService) AddMany(ctx context.Context, nodes []ipld.Node) error {
	// No-op for this simple mock
	return nil
}

func (m *mockDagService) Get(ctx context.Context, c cid.Cid) (ipld.Node, error) {
	// Return ErrNotFound or a predefined node if needed for specific tests
	return nil, ipld.ErrNotFound{Cid: c}
}

func (m *mockDagService) GetMany(ctx context.Context, cids []cid.Cid) <-chan *ipld.NodeOption {
	// Return a channel that immediately closes or sends ErrNotFound
	ch := make(chan *ipld.NodeOption, 1)
	defer close(ch)
	// Or loop through cids and send NodeOption with ErrNotFound for each
	return ch
}

func (m *mockDagService) Remove(ctx context.Context, c cid.Cid) error {
	// No-op for this simple mock
	return nil
}

func (m *mockDagService) RemoveMany(ctx context.Context, cids []cid.Cid) error {
	// No-op for this simple mock
	return nil
}

// setupTestStores creates in-memory datastores for testing.
// It returns a CRDT store and a base datastore.
func setupTestStores(t *testing.T) (*crdt.Datastore, datastore.Datastore) {
	// Base store (simple map datastore)
	baseStore := dsync.MutexWrap(datastore.NewMapDatastore())

	// Minimal DAGService mock needed for crdt.New
	mockDag := &mockDagService{}

	// CRDT store setup (requires a base store and a broadcaster)
	// For this test, we don't need real pubsub, so a nil broadcaster is acceptable
	// as we are testing local Save/Load, not replication.
	crdtOpts := crdt.DefaultOptions()
	crdtStore, err := crdt.New(baseStore, datastore.NewKey("crdt"), mockDag, nil /* broadcaster */, crdtOpts)
	require.NoError(t, err, "Failed to create CRDT store")

	t.Cleanup(func() {
		// Ensure CRDT store is closed cleanly
		crdtStore.Close()
	})

	return crdtStore, baseStore
}

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

	return &types.FlowObject{
		ID:         objCid,
		ObjectType: "test-object",
		Metadata:   meta,
		State:      state,
		Sequence:   1,
	}
}

func TestSaveLoadFlowObject(t *testing.T) {
	ctx := context.Background()
	crdtStore, baseStore := setupTestStores(t)

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

		err := SaveFlowObject(ctx, crdtStore, baseStore, originalObj)
		require.NoError(t, err, "SaveFlowObject with empty state failed")

		loadedObj, err := LoadFlowObject(ctx, crdtStore, baseStore, originalObj.ID)
		require.NoError(t, err, "LoadFlowObject with empty state failed")
		require.NotNil(t, loadedObj)
		require.Equal(t, originalObj.ID, loadedObj.ID)
		// When state is saved as `[]byte{}`, it should be loaded back as non-nil empty slice.
		require.NotNil(t, loadedObj.State, "Expected loaded state to be non-nil when saved as empty slice")
		require.Len(t, loadedObj.State, 0, "Expected loaded state to be empty slice")
		require.Equal(t, originalObj.Metadata, loadedObj.Metadata)
	})

	// TODO: Add tests for potential inconsistencies? (e.g., meta exists, state doesn't - although Load handles this)
}
