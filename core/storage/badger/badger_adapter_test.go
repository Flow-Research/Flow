package badger_test

import (
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	storage "flow-network/flow/core/storage"
	"flow-network/flow/core/storage/badger"
	types "flow-network/flow/core/storage/types"
	cid "github.com/ipfs/go-cid"
)

// setupTestDB creates a temporary directory and initializes a BadgerAdapter for testing.
// It returns the adapter and a cleanup function to be deferred.
func setupTestDB(t *testing.T) (storage.StorageAdapter, func()) {
	t.Helper()

	tempDir, err := os.MkdirTemp("", "badger_test_*")
	require.NoError(t, err, "Failed to create temp dir")

	adapter, err := badger.NewBadgerAdapter(badger.BadgerOptions{Path: tempDir})
	require.NoError(t, err, "Failed to create BadgerAdapter")

	cleanup := func() {
		adapter.Close()
		os.RemoveAll(tempDir)
	}

	return adapter, cleanup
}

// TestCreateLoadObject verifies the basic cycle of creating an object and loading it back.
func TestCreateLoadObject(t *testing.T) {
	adapter, cleanup := setupTestDB(t)
	defer cleanup()

	// --- Test Data ---
	objectType := "test_document"
	metadata := map[string]interface{}{"name": "test_doc_1", "version": 1.0}
	initialState := []byte(`{"content":"initial state"}`)

	// --- Create Object ---
	objId, err := adapter.CreateObject(objectType, metadata, initialState)

	// --- Assert Create ---
	require.NoError(t, err, "CreateObject should not return an error")
	assert.NotEqual(t, types.UndefObjectId, objId, "CreateObject should return a valid ObjectId (CID)")
	assert.NotEqual(t, cid.Undef, objId, "ObjectId should not be undefined")

	// --- Load Object ---
	loadedObj, err := adapter.LoadObject(objId)

	// --- Assert Load ---
	require.NoError(t, err, "LoadObject should not return an error for a created object")
	require.NotNil(t, loadedObj, "LoadObject should return a non-nil FlowObject")

	// --- Assert Object Content ---
	// The loaded object should reflect the state at creation time
	assert.Equal(t, objectType, loadedObj.ObjectType, "Loaded object type should match")
	assert.Equal(t, metadata, loadedObj.Metadata, "Loaded object metadata should match")
	assert.Equal(t, initialState, loadedObj.CRDTState, "Loaded object state should match initial state")
	assert.Equal(t, uint64(0), loadedObj.Sequence, "Newly created object sequence should be 0")

}

// TestApplyDeltaAndHistory verifies applying deltas and retrieving history.
func TestApplyDeltaAndHistory(t *testing.T) {
	adapter, cleanup := setupTestDB(t)
	defer cleanup()

	// --- Initial Object ---
	objId, err := adapter.CreateObject("delta_test", nil, []byte(`"initial"`))
	require.NoError(t, err)
	require.NotEqual(t, types.UndefObjectId, objId, "CreateObject should return a valid ObjectId (CID)")
	require.NotEqual(t, cid.Undef, objId, "ObjectId should not be undefined")

	// --- Delta 1 ---
	delta1 := &types.Delta{
		Payload:   []byte(`"update 1"`),
		AuthorDID: "did:example:author1",
		Timestamp: uint64(time.Now().UnixNano()),
	}
	err = adapter.ApplyDelta(objId, delta1)
	require.NoError(t, err, "ApplyDelta 1 should succeed")

	// --- Load After Delta 1 ---
	loadedObj1, err := adapter.LoadObject(objId)
	require.NoError(t, err)
	assert.Equal(t, uint64(1), loadedObj1.Sequence, "Sequence should be 1 after delta 1")
	assert.Equal(t, delta1.Payload, loadedObj1.CRDTState, "State should match delta 1 payload (simulated)")

	// --- Delta 2 ---
	delta2 := &types.Delta{
		Payload:   []byte(`"update 2"`),
		AuthorDID: "did:example:author2",
		Timestamp: uint64(time.Now().UnixNano()),
	}
	err = adapter.ApplyDelta(objId, delta2)
	require.NoError(t, err, "ApplyDelta 2 should succeed")

	// --- Load After Delta 2 ---
	loadedObj2, err := adapter.LoadObject(objId)
	require.NoError(t, err)
	assert.Equal(t, uint64(2), loadedObj2.Sequence, "Sequence should be 2 after delta 2")
	assert.Equal(t, delta2.Payload, loadedObj2.CRDTState, "State should match delta 2 payload (simulated)")

	// --- Get History ---
	history, err := adapter.GetHistory(objId)
	require.NoError(t, err, "GetHistory should succeed")
	require.Len(t, history, 2, "History should contain 2 deltas")

	// --- Assert History Content (order matters) ---
	// Note: The history contains pointers to Delta structs
	assert.Equal(t, delta1.Payload, history[0].Payload, "History[0] payload should match delta 1")
	assert.Equal(t, delta1.AuthorDID, history[0].AuthorDID, "History[0] AuthorDID should match delta 1")
	// Timestamps might be slightly different due to test execution time, only check payload/author

	assert.Equal(t, delta2.Payload, history[1].Payload, "History[1] payload should match delta 2")
	assert.Equal(t, delta2.AuthorDID, history[1].AuthorDID, "History[1] AuthorDID should match delta 2")
}

// TestCreateLoadSnapshot verifies creating and loading object snapshots.
func TestCreateLoadSnapshot(t *testing.T) {
	adapter, cleanup := setupTestDB(t)
	defer cleanup()

	// --- Initial Object ---
	initialState := []byte(`"state v0"`)
	objId, err := adapter.CreateObject("snapshot_test", map[string]interface{}{"version": 0}, initialState)
	require.NoError(t, err)
	require.NotEqual(t, types.UndefObjectId, objId, "CreateObject should return a valid ObjectId (CID)")
	require.NotEqual(t, cid.Undef, objId, "ObjectId should not be undefined")

	// --- Apply Delta (to make snapshot state different from initial) ---
	delta1 := &types.Delta{
		Payload:   []byte(`"state v1"`),
		AuthorDID: "did:example:snap",
		Timestamp: uint64(time.Now().UnixNano()),
	}
	err = adapter.ApplyDelta(objId, delta1)
	require.NoError(t, err)

	// --- Get Expected Snapshot State (after delta) ---
	expectedObj, err := adapter.LoadObject(objId)
	require.NoError(t, err, "Failed to load object state before snapshot creation")
	require.Equal(t, uint64(1), expectedObj.Sequence, "Sequence should be 1 before snapshot")

	// --- Create Snapshot ---
	snapshotId, err := adapter.CreateSnapshot(objId)

	// --- Assert Snapshot Creation ---
	require.NoError(t, err, "CreateSnapshot should not return an error")
	assert.NotEqual(t, types.UndefSnapshotId, snapshotId, "CreateSnapshot should return a valid SnapshotId (CID)")
	assert.NotEqual(t, cid.Undef, snapshotId, "SnapshotId should not be undefined")

	// --- Load Snapshot ---
	loadedSnapshot, err := adapter.LoadSnapshot(snapshotId)

	// --- Assert Snapshot Load ---
	require.NoError(t, err, "LoadSnapshot should not return an error for a created snapshot")
	require.NotNil(t, loadedSnapshot, "LoadSnapshot should return a non-nil FlowObject")

	// --- Assert Snapshot Content ---
	// The loaded snapshot should reflect the state *when the snapshot was taken* (after delta1)
	assert.Equal(t, expectedObj.ID, loadedSnapshot.ID, "Snapshot object ID should match original object ID")
	assert.Equal(t, expectedObj.ObjectType, loadedSnapshot.ObjectType, "Snapshot object type should match")
	assert.Equal(t, expectedObj.Metadata, loadedSnapshot.Metadata, "Snapshot metadata should match")
	assert.Equal(t, expectedObj.CRDTState, loadedSnapshot.CRDTState, "Snapshot CRDT state should match state at snapshot time")
	assert.Equal(t, expectedObj.Sequence, loadedSnapshot.Sequence, "Snapshot sequence should match sequence at snapshot time")

	// --- Verify Current Object State (Optional, ensure snapshot didn't change live object) ---
	currentObj, err := adapter.LoadObject(objId)
	require.NoError(t, err)
	assert.Equal(t, expectedObj.CRDTState, currentObj.CRDTState, "Current object state should remain unchanged after snapshot")
	assert.Equal(t, expectedObj.Sequence, currentObj.Sequence, "Current object sequence should remain unchanged after snapshot")
}

// TODO: Add tests for CreateSnapshot, LoadSnapshot
